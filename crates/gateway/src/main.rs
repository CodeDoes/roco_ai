//! RoCo AI — gateway server (Phase 3).
//!
//! An axum HTTP server that exposes an RPC endpoint for remote
//! clients. Accepts task submissions and streams/serves traces.
//!
//! Endpoints:
//!   POST /rpc           — run a task, return the trace
//!   GET  /traces        — list saved traces
//!   GET  /trace/:id     — get a full trace by ID
//!   GET  /trace/:id/stream — SSE-stream the events of a saved trace
//!   GET  /health        — health check

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        Json,
    },
    routing::{get, post},
    Router,
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};

use tracing::info;

use roco_core::agent::{ChecklistVerifier, ContextBudget, Orchestrator, RetryPolicy, Task};
use roco_core::engine::MockBackend;
use roco_core::trace::{CollectingTracer, Trace, TraceStore, TraceSummary};

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

struct AppState {
    store: TraceStore,
    backend: Arc<MockBackend>,
}

impl AppState {
    fn new() -> Self {
        Self {
            store: TraceStore::new(".roco/traces"),
            backend: Arc::new(MockBackend {
                name: "mock-3b".into(),
                ..Default::default()
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RunTaskRequest {
    objective: String,
    #[serde(default)]
    context: String,
    #[serde(default = "default_schema")]
    output_schema: String,
    #[serde(default = "default_true")]
    allow_abstain: bool,
}

fn default_schema() -> String {
    r#"{"result": "<string>"}"#.into()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
struct TraceListEntry {
    id: String,
    objective: String,
    events: usize,
    subtasks: usize,
    failed: usize,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    service: String,
    version: &'static str,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /rpc — run a task and return the full trace.
async fn run_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RunTaskRequest>,
) -> Result<Json<Trace>, (StatusCode, String)> {
    let tracer = CollectingTracer::new();

    let task = Task {
        id: format!("gateway-{}", chrono_id()),
        objective: req.objective,
        context: req.context,
        output_schema: req.output_schema,
        allow_abstain: req.allow_abstain,
    };

    let orchestrator = Orchestrator::new(
        Arc::clone(&state.backend),
        ContextBudget::default(),
        ChecklistVerifier,
        RetryPolicy::default(),
    )
    .with_tracer(Arc::new(tracer.clone()));

    let result = orchestrator
        .run(&task)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let events = tracer.snapshot();
    let model_calls = events.iter().filter(|e| e.phase == "model_call").count();
    let tool_calls = events
        .iter()
        .filter(|e| e.phase == "tool_parse" && !e.detail.contains("final answer"))
        .count();
    let retries = events.iter().filter(|e| e.phase == "retry").count();
    let first_ts = events.first().map(|e| e.ts_ms).unwrap_or(0);
    let last_ts = events.last().map(|e| e.ts_ms).unwrap_or(0);

    let trace_id = format!("gw-{}", chrono_id());

    let messages = serde_json::json!([
        { "role": "user", "content": &task.objective },
        {
            "role": "assistant",
            "content": format!(
                "{} subtasks executed, {} failed. {} events recorded.",
                result.subtask_count, result.failed, events.len()
            ),
        },
    ]);
    let memory = serde_json::Value::Null;

    let trace = Trace::from_collector(
        &trace_id,
        &task.objective,
        &tracer,
        messages,
        memory,
        TraceSummary {
            subtask_count: result.subtask_count,
            failed_subtasks: result.failed,
            model_calls,
            tool_calls,
            tool_errors: 0,
            retries,
            duration_ms: last_ts.saturating_sub(first_ts),
        },
    );

    // Save the trace for later retrieval / streaming
    if let Err(e) = state.store.save(&trace) {
        tracing::warn!("failed to save trace: {e}");
    }

    Ok(Json(trace))
}

/// GET /traces — list saved traces.
async fn list_traces(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<TraceListEntry>> {
    let ids = state.store.list().unwrap_or_default();
    let mut entries = Vec::new();
    for id in &ids {
        if let Ok(t) = state.store.load(id) {
            entries.push(TraceListEntry {
                id: t.id,
                objective: t.objective,
                events: t.events.len(),
                subtasks: t.summary.subtask_count,
                failed: t.summary.failed_subtasks,
            });
        }
    }
    Json(entries)
}

/// GET /trace/:id — get a full trace.
async fn get_trace(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Trace>, (StatusCode, String)> {
    let trace = state
        .store
        .load(&id)
        .map_err(|e| (StatusCode::NOT_FOUND, format!("trace '{id}' not found: {e}")))?;
    Ok(Json(trace))
}

/// GET /trace/:id/stream — SSE-stream the events of a saved trace.
async fn stream_trace(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>>, (StatusCode, String)> {
    let trace = state
        .store
        .load(&id)
        .map_err(|e| (StatusCode::NOT_FOUND, format!("trace '{id}' not found: {e}")))?;

    let events = trace.events;

    let stream = stream::iter(
        events
            .into_iter()
            .map(|ev| {
                let json = serde_json::to_string(&ev).unwrap_or_default();
                Ok(Event::default().data(json))
            })
            .chain(std::iter::once(Ok(
                Event::default().data("[DONE]").event("done")
            ))),
    );

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// GET /health — health check.
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        service: "roco-gateway".into(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Generate a timestamp-based ID.
fn chrono_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let state = Arc::new(AppState::new());

    let app = Router::new()
        .route("/rpc", post(run_task))
        .route("/traces", get(list_traces))
        .route("/trace/{id}", get(get_trace))
        .route("/trace/{id}/stream", get(stream_trace))
        .route("/health", get(health))
        .with_state(state);

    let addr = "0.0.0.0:3001";
    info!("RoCo AI gateway listening on {addr}");
    info!("  POST /rpc          — run a task");
    info!("  GET  /traces       — list traces");
    info!("  GET  /trace/:id    — get trace");
    info!("  GET  /trace/:id/stream — SSE stream trace events");
    info!("  GET  /health       — health check");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
