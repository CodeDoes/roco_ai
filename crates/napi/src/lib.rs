//! RoCo AI — napi-rs bindings (Phase 2).
//!
//! Exposes the core orchestrator as async Node.js functions:
//! - `runTask(input) -> String` — run a task, returns JSON trace
//! - `listTraces() -> String` — returns JSON array of trace IDs with summaries
//! - `loadTrace(id) -> String` — returns JSON trace
//! - `diffTraces(id1, id2) -> String` — returns JSON trace diff
//!
//! Usage from Node.js:
//! ```js
//! import { runTask, listTraces, loadTrace, diffTraces } from './roco_napi';
//! const trace = JSON.parse(await runTask({ objective: "...", context: "..." }));
//! ```

use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Deserialize;

use roco_core::agent::{ChecklistVerifier, ContextBudget, Orchestrator, RetryPolicy, Task};
use roco_core::engine::MockBackend;
use roco_core::trace::{CollectingTracer, Trace, TraceStore, TraceSummary};

// ---------------------------------------------------------------------------
// Input types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[napi(object)]
pub struct RunTaskInput {
    pub objective: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub output_schema: String,
    #[serde(default = "default_allow_abstain")]
    pub allow_abstain: bool,
}

fn default_allow_abstain() -> bool {
    true
}

// ---------------------------------------------------------------------------
// NAPI functions — all return JSON strings for simplicity
// ---------------------------------------------------------------------------

#[napi]
pub async fn run_task(input: RunTaskInput) -> Result<String> {
    let backend = Arc::new(MockBackend {
        name: "mock-3b".into(),
        ..Default::default()
    });
    let budget = ContextBudget::default();
    let tracer = CollectingTracer::new();

    let task = Task {
        id: "napi-task".into(),
        objective: input.objective.clone(),
        context: input.context.clone(),
        output_schema: if input.output_schema.is_empty() {
            r#"{"result": "<string>"}"#.into()
        } else {
            input.output_schema.clone()
        },
        allow_abstain: input.allow_abstain,
    };

    let orchestrator = Orchestrator::new(
        backend,
        budget,
        ChecklistVerifier,
        RetryPolicy::default(),
    )
    .with_tracer(Arc::new(tracer.clone()));

    let result = orchestrator
        .run(&task)
        .await
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;

    let trace_id = format!(
        "napi-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    let messages = serde_json::json!([
        { "role": "user", "content": &task.objective },
        { "role": "assistant", "content": format!("{} subtasks, {} failed", result.subtask_count, result.failed) }
    ]);
    let memory = serde_json::Value::Null;

    let events = tracer.snapshot();
    let model_calls = events.iter().filter(|e| e.phase == "model_call").count();
    let tool_calls = events.iter().filter(|e| e.phase == "tool_parse" && !e.detail.contains("final answer")).count();
    let retries = events.iter().filter(|e| e.phase == "retry").count();
    let first_ts = events.first().map(|e| e.ts_ms).unwrap_or(0);
    let last_ts = events.last().map(|e| e.ts_ms).unwrap_or(0);

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

    // Auto-save to trace store
    let store = TraceStore::new(".roco/traces");
    if let Err(e) = store.save(&trace) {
        eprintln!("warn: failed to save trace: {e}");
    }

    serde_json::to_string_pretty(&trace)
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
}

#[napi]
pub fn list_traces() -> Result<String> {
    let store = TraceStore::new(".roco/traces");
    let ids = store.list().map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
    let mut entries = Vec::new();
    for id in &ids {
        if let Ok(t) = store.load(id) {
            entries.push(serde_json::json!({
                "id": id,
                "objective": t.objective,
                "events": t.events.len(),
                "subtasks": t.summary.subtask_count,
                "failed": t.summary.failed_subtasks,
            }));
        }
    }
    serde_json::to_string_pretty(&entries)
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
}

#[napi]
pub fn load_trace(id: String) -> Result<String> {
    let store = TraceStore::new(".roco/traces");
    let trace = store.load(&id).map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
    serde_json::to_string_pretty(&trace)
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
}

#[napi]
pub fn diff_traces(id1: String, id2: String) -> Result<String> {
    let store = TraceStore::new(".roco/traces");
    let t1 = store.load(&id1).map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
    let t2 = store.load(&id2).map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
    let diff = TraceStore::diff(&id1, &t1, &id2, &t2);
    serde_json::to_string_pretty(&diff)
        .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
}
