//! RoCo Gateway — Session, Workspace, and Job Orchestrator.
//!
//! The gateway is NOT just a rate-limiting proxy. It is the central
//! orchestrator that manages:
//! - Session lifecycle (create, bake, generate, archive)
//! - Workspace file operations (sandboxed, versioned)
//! - Background job queue (survives client disconnect)
//! - Message streaming (SSE fan-out, resume)
//!
//! The gateway speaks to inferd (pure inference) and exposes a rich
//! API to clients (CLI, server, UI, external tools).

pub mod job;
pub mod session;
pub mod workspace;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, Request, State},
    response::{IntoResponse, Response, Sse},
    routing::{delete, get, post},
    Json, Router as AxumRouter,
};
use parking_lot::Mutex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use job::{JobEvent, JobQueue};
use session::{SessionManager, SessionStatus};
use workspace::WorkspaceManager;

// ── Gateway State ──────────────────────────────────────────────────────

#[derive(Clone)]
pub struct GatewayState {
    /// HTTP client for talking to inferd.
    pub inferd_client: Client,
    /// Inferd base URL.
    pub inferd_url: String,
    /// Session manager.
    pub sessions: Arc<SessionManager>,
    /// Workspace manager.
    pub workspaces: Arc<WorkspaceManager>,
    /// Job queue for background generation.
    pub jobs: Arc<JobQueue>,
    /// Rate limiter (secondary concern).
    pub rate_limiter: Arc<Mutex<std::collections::HashMap<String, Vec<Instant>>>>,
    pub rate_limit_per_minute: usize,
}

// ── Gateway ─────────────────────────────────────────────────────────────

pub struct Gateway {
    pub host: String,
    pub port: u16,
    pub inferd_url: String,
    pub workspace_dir: std::path::PathBuf,
    pub rate_limit_per_minute: usize,
}

impl Default for Gateway {
    fn default() -> Self {
        Self::new(
            "127.0.0.1".to_string(),
            8000,
            "http://127.0.0.1:8080".to_string(),
            std::path::PathBuf::from("./workspaces"),
            60,
        )
    }
}

impl Gateway {
    pub fn new(
        host: String,
        port: u16,
        inferd_url: String,
        workspace_dir: std::path::PathBuf,
        rate_limit_per_minute: usize,
    ) -> Self {
        Self {
            host,
            port,
            inferd_url,
            workspace_dir,
            rate_limit_per_minute,
        }
    }

    pub async fn run(&self) -> Result<(), String> {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .try_init();

        let state = GatewayState {
            inferd_client: Client::new(),
            inferd_url: self.inferd_url.clone(),
            sessions: Arc::new(SessionManager::new()),
            workspaces: Arc::new(WorkspaceManager::new(&self.workspace_dir)),
            jobs: Arc::new(JobQueue::new()),
            rate_limiter: Arc::new(Mutex::new(std::collections::HashMap::new())),
            rate_limit_per_minute: self.rate_limit_per_minute,
        };

        // Spawn background job worker
        let worker_state = state.clone();
        tokio::spawn(async move {
            job_worker(worker_state).await;
        });

        let app = AxumRouter::new()
            // Health
            .route("/health", get(handle_health))
            // Sessions
            .route("/sessions", post(handle_create_session))
            .route("/sessions/:id", get(handle_get_session))
            .route("/sessions/:id", delete(handle_delete_session))
            .route("/sessions/:id/bake", post(handle_bake_session))
            .route("/sessions/:id/complete", post(handle_complete_session))
            .route("/sessions/:id/eos", post(handle_feed_eos))
            .route("/sessions/:id/tokens", get(handle_get_tokens))
            .route("/sessions/:id/status", get(handle_get_session_status))
            // Workspaces
            .route("/workspaces", get(handle_list_workspaces))
            .route("/workspaces", post(handle_create_workspace))
            .route("/workspaces/:id", get(handle_get_workspace))
            .route("/workspaces/:id/files", get(handle_list_files))
            .route("/workspaces/:id/files/*path", get(handle_read_file))
            .route("/workspaces/:id/files/*path", post(handle_write_file))
            // Jobs
            .route("/jobs", post(handle_create_job))
            .route("/jobs/:id", get(handle_get_job))
            .route("/jobs/:id/stream", get(handle_stream_job))
            .route("/jobs/:id/cancel", post(handle_cancel_job))
            // Proxy everything else to inferd
            .fallback(handle_proxy_to_inferd)
            .with_state(state);

        let addr = format!("{}:{}", self.host, self.port);
        info!(
            "Starting RoCo Gateway on {} → inferd at {}",
            addr, self.inferd_url
        );
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("Failed to bind gateway to {addr}: {e}"))?;
        axum::serve(listener, app)
            .await
            .map_err(|e| format!("Gateway run error: {e}"))?;
        Ok(())
    }
}

// ── Background Job Worker ────────────────────────────────────────────────

async fn job_worker(state: GatewayState) {
    let mut rx = state.jobs.subscribe();
    loop {
        match rx.recv().await {
            Ok(JobEvent::Started { job_id }) => {
                info!("Job {} started", job_id);
            }
            Ok(JobEvent::Token { job_id, token }) => {
                // Write token to workspace file for persistence
                if let Some(job) = state.jobs.get(&job_id) {
                    let path = format!("jobs/{}/output.md", job_id);
                    let _ = state
                        .workspaces
                        .write_file(&job.workspace_id, &path, &token);
                }
            }
            Ok(JobEvent::Completed { job_id }) => {
                info!("Job {} completed", job_id);
            }
            Ok(JobEvent::Failed { job_id, error }) => {
                warn!("Job {} failed: {}", job_id, error);
            }
            Ok(JobEvent::Cancelled { job_id }) => {
                info!("Job {} cancelled", job_id);
            }
            Err(_) => break,
        }
    }
}

// ── Session Handlers ────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct CreateSessionResponse {
    session_id: String,
    workspace_id: String,
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    workspace_id: Option<String>,
}

async fn handle_create_session(
    State(state): State<GatewayState>,
    Json(req): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let ws_id = req.workspace_id.unwrap_or_else(|| "default".into());
    // Ensure workspace exists
    if state.workspaces.get(&ws_id).is_none() {
        let _ = state.workspaces.create(&ws_id);
    }
    let session_id = state.sessions.create(&ws_id);
    Json(CreateSessionResponse {
        session_id,
        workspace_id: ws_id,
    })
}

async fn handle_get_session(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.sessions.get(&id) {
        Some(session) => Json(session).into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "Session not found").into_response(),
    }
}

async fn handle_delete_session(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    state.sessions.delete(&id);
    axum::http::StatusCode::NO_CONTENT
}

#[derive(Debug, Deserialize)]
struct BakeRequest {
    system: String,
    few_shots: Vec<(String, String)>,
}

async fn handle_bake_session(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
    Json(req): Json<BakeRequest>,
) -> impl IntoResponse {
    // Forward to inferd
    let url = format!("{}/bake", state.inferd_url);
    let body = serde_json::json!({
        "session_id": id,
        "system": req.system,
        "few_shots": req.few_shots,
    });

    match state.inferd_client.post(&url).json(&body).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                state.sessions.update(&id, |s| {
                    s.system_prompt = req.system;
                    s.baked_shots = req.few_shots.len();
                });
                axum::http::StatusCode::OK.into_response()
            } else {
                (axum::http::StatusCode::BAD_GATEWAY, "Inferd bake failed").into_response()
            }
        }
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Inferd unreachable: {e}"),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct CompleteRequest {
    prompt: String,
    #[serde(default)]
    stream: bool,
    #[serde(default = "default_temperature")]
    temperature: f32,
    #[serde(default = "default_max_tokens")]
    max_tokens: usize,
    grammar: Option<String>,
}

fn default_temperature() -> f32 {
    0.7
}
fn default_max_tokens() -> usize {
    512
}

async fn handle_complete_session(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
    Json(req): Json<CompleteRequest>,
) -> impl IntoResponse {
    // Check session exists
    if state.sessions.get(&id).is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "Session not found").into_response();
    }

    if req.stream {
        // For streaming, create a job and return job ID
        let job_id = state.jobs.submit(&id, "default", &req.prompt);
        state.jobs.update(&job_id, |j| {
            j.grammar = req.grammar.clone();
            j.temperature = req.temperature;
            j.max_tokens = req.max_tokens;
        });

        // Start the job in background
        let state_clone = state.clone();
        let job_id_clone = job_id.clone();
        tokio::spawn(async move {
            run_job_on_inferd(state_clone, job_id_clone).await;
        });

        Json(serde_json::json!({ "job_id": job_id, "status": "queued" })).into_response()
    } else {
        // Synchronous completion
        let url = format!("{}/complete", state.inferd_url);
        let body = serde_json::json!({
            "prompt": req.prompt,
            "session": id,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
            "grammar": req.grammar,
        });

        match state.inferd_client.post(&url).json(&body).send().await {
            Ok(resp) => {
                let text = resp.text().await.unwrap_or_default();
                // Update session with result
                state.sessions.update(&id, |s| {
                    s.status = SessionStatus::Completed;
                    s.accumulated_tokens.push(text.clone());
                });
                Json(serde_json::json!({ "text": text })).into_response()
            }
            Err(e) => (
                axum::http::StatusCode::BAD_GATEWAY,
                format!("Inferd error: {e}"),
            )
                .into_response(),
        }
    }
}

async fn handle_feed_eos(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let url = format!("{}/feed_eos", state.inferd_url);
    let body = serde_json::json!({ "session_id": id });

    match state.inferd_client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => axum::http::StatusCode::OK.into_response(),
        _ => (axum::http::StatusCode::BAD_GATEWAY, "Failed to feed EOS").into_response(),
    }
}

async fn handle_get_tokens(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.sessions.get_tokens(&id) {
        Some(tokens) => Json(tokens).into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "Session not found").into_response(),
    }
}

async fn handle_get_session_status(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.sessions.get(&id) {
        Some(session) => Json(serde_json::json!({
            "session_id": session.id,
            "status": session.status,
            "token_count": session.accumulated_tokens.len(),
            "baked_shots": session.baked_shots,
        }))
        .into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "Session not found").into_response(),
    }
}

// ── Workspace Handlers ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct WorkspaceListResponse {
    workspaces: Vec<workspace::Workspace>,
}

async fn handle_list_workspaces(State(state): State<GatewayState>) -> impl IntoResponse {
    Json(WorkspaceListResponse {
        workspaces: state.workspaces.list_all(),
    })
}

#[derive(Debug, Deserialize)]
struct CreateWorkspaceRequest {
    id: Option<String>,
}

async fn handle_create_workspace(
    State(state): State<GatewayState>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> impl IntoResponse {
    let id = req
        .id
        .unwrap_or_else(|| format!("ws-{}", uuid::Uuid::new_v4()));
    match state.workspaces.create(&id) {
        Ok(ws) => Json(ws).into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

async fn handle_get_workspace(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.workspaces.get(&id) {
        Some(ws) => Json(ws).into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "Workspace not found").into_response(),
    }
}

async fn handle_list_files(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.workspaces.list_files(&id) {
        Ok(files) => Json(files).into_response(),
        Err(e) => (axum::http::StatusCode::NOT_FOUND, e).into_response(),
    }
}

async fn handle_read_file(
    State(state): State<GatewayState>,
    Path((id, path)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.workspaces.read_file(&id, &path) {
        Ok(content) => content.into_response(),
        Err(e) => (axum::http::StatusCode::NOT_FOUND, e).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct WriteFileRequest {
    content: String,
}

async fn handle_write_file(
    State(state): State<GatewayState>,
    Path((id, path)): Path<(String, String)>,
    Json(req): Json<WriteFileRequest>,
) -> impl IntoResponse {
    match state.workspaces.write_file(&id, &path, &req.content) {
        Ok(()) => axum::http::StatusCode::OK.into_response(),
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

// ── Job Handlers ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateJobRequest {
    session_id: String,
    prompt: String,
    #[serde(default = "default_temperature")]
    temperature: f32,
    #[serde(default = "default_max_tokens")]
    max_tokens: usize,
    grammar: Option<String>,
}

async fn handle_create_job(
    State(state): State<GatewayState>,
    Json(req): Json<CreateJobRequest>,
) -> impl IntoResponse {
    let job_id = state.jobs.submit(&req.session_id, "default", &req.prompt);
    state.jobs.update(&job_id, |j| {
        j.grammar = req.grammar;
        j.temperature = req.temperature;
        j.max_tokens = req.max_tokens;
    });

    // Start the job in background
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        run_job_on_inferd(state_clone, job_id_clone).await;
    });

    Json(serde_json::json!({ "job_id": job_id, "status": "queued" }))
}

async fn handle_get_job(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.jobs.get(&id) {
        Some(job) => Json(job).into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "Job not found").into_response(),
    }
}

async fn handle_stream_job(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // SSE stream of job tokens
    let mut rx = state.jobs.subscribe();
    let job_id = id.clone();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(JobEvent::Token { job_id: jid, token }) if jid == job_id => {
                    yield Ok::<_, std::convert::Infallible>(
                        axum::response::sse::Event::default().data(token)
                    );
                }
                Ok(JobEvent::Completed { job_id: jid }) if jid == job_id => {
                    yield Ok(axum::response::sse::Event::default().data("[DONE]"));
                    break;
                }
                Ok(JobEvent::Failed { job_id: jid, error }) if jid == job_id => {
                    yield Ok(axum::response::sse::Event::default().data(format!("[ERROR] {}", error)));
                    break;
                }
                _ => {}
            }
        }
    };

    Sse::new(stream)
}

async fn handle_cancel_job(
    State(state): State<GatewayState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    state.jobs.cancel(&id);
    axum::http::StatusCode::OK
}

// ── Inferd Proxy (fallback) ──────────────────────────────────────────────

async fn handle_proxy_to_inferd(State(state): State<GatewayState>, req: Request) -> Response {
    let method = req.method().clone();
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or_else(|| req.uri().path())
        .to_string();
    let forward_url = format!(
        "{}{}",
        state.inferd_url.trim_end_matches('/'),
        path_and_query
    );

    let client_ip = "global".to_string();
    let start = Instant::now();

    // Rate limiting (secondary)
    let is_health = path_and_query == "/health" || path_and_query.starts_with("/vocab");
    if !is_health {
        let mut limiter = state.rate_limiter.lock();
        let now = Instant::now();
        let timestamps = limiter.entry(client_ip.clone()).or_insert_with(Vec::new);
        timestamps.retain(|&t| now.duration_since(t) < Duration::from_secs(60));
        const MAX_HISTORY: usize = 10_000;
        if timestamps.len() > MAX_HISTORY {
            timestamps.drain(..timestamps.len() - MAX_HISTORY);
        }
        if timestamps.len() >= state.rate_limit_per_minute {
            warn!("Rate limit exceeded for {}", client_ip);
            return (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "Rate limit exceeded. Try again later.",
            )
                .into_response();
        }
        timestamps.push(now);
    }

    let body_bytes = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("Failed to read request body: {}", e);
            return (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Failed to read request body: {}", e),
            )
                .into_response();
        }
    };

    let mut forwarded = state.inferd_client.request(method, &forward_url);
    forwarded = forwarded.body(body_bytes);

    let response = match forwarded.send().await {
        Ok(resp) => resp,
        Err(e) => {
            warn!("Failed to forward request to inferd: {}", e);
            return (
                axum::http::StatusCode::BAD_GATEWAY,
                format!("Failed to forward request to inferd: {}", e),
            )
                .into_response();
        }
    };

    let status = response.status();
    let headers = response.headers().clone();
    let body = match response.bytes().await {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to read inferd response body: {}", e);
            return (
                axum::http::StatusCode::BAD_GATEWAY,
                format!("Failed to read inferd response: {}", e),
            )
                .into_response();
        }
    };

    let elapsed = start.elapsed();
    info!(
        "{} {} → {} ({:?})",
        client_ip,
        path_and_query,
        status.as_u16(),
        elapsed
    );

    let mut builder = Response::builder().status(status);
    for (key, value) in headers.iter() {
        if key.as_str() != "content-encoding" {
            builder = builder.header(key, value);
        }
    }
    builder
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|e| {
            warn!("Failed to build response: {}", e);
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to build response",
            )
                .into_response()
        })
}

// ── Health ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    gateway: String,
    inferd: String,
    active_sessions: usize,
    active_jobs: usize,
    workspaces: usize,
}

async fn handle_health(State(state): State<GatewayState>) -> impl IntoResponse {
    // Check inferd health
    let inferd_status = match state
        .inferd_client
        .get(format!("{}/health", state.inferd_url))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => "healthy".to_string(),
        _ => "unreachable".to_string(),
    };

    Json(HealthResponse {
        status: "healthy".into(),
        gateway: env!("CARGO_PKG_VERSION").into(),
        inferd: inferd_status,
        active_sessions: state.sessions.list_all().len(),
        active_jobs: state.jobs.list_for_session("").len(), // TODO: fix this
        workspaces: state.workspaces.list_all().len(),
    })
}

// ── Job Runner (background task) ───────────────────────────────────────

async fn run_job_on_inferd(state: GatewayState, job_id: String) {
    let job = match state.jobs.get(&job_id) {
        Some(j) => j,
        None => return,
    };

    state.jobs.start(&job_id);
    state
        .sessions
        .set_status(&job.session_id, SessionStatus::Generating);

    let url = format!("{}/complete", state.inferd_url);
    let body = serde_json::json!({
        "prompt": job.prompt,
        "session": job.session_id,
        "temperature": job.temperature,
        "max_tokens": job.max_tokens,
        "grammar": job.grammar,
        "stream": true,
    });

    match state.inferd_client.post(&url).json(&body).send().await {
        Ok(resp) => {
            let mut stream = resp.bytes_stream();
            use futures_util::StreamExt;
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            // Parse SSE format: data: {...}
                            for line in text.lines() {
                                if let Some(json_str) = line.strip_prefix("data: ") {
                                    if let Ok(event) =
                                        serde_json::from_str::<serde_json::Value>(json_str)
                                    {
                                        if let Some(token) = event["choices"][0]["text"].as_str() {
                                            state.jobs.append_token(&job_id, token);
                                            state
                                                .sessions
                                                .append_tokens(&job.session_id, vec![token.into()]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        state.jobs.fail(&job_id, format!("stream error: {e}"));
                        state
                            .sessions
                            .set_status(&job.session_id, SessionStatus::Error);
                        return;
                    }
                }
            }
            state.jobs.complete(&job_id);
            state
                .sessions
                .set_status(&job.session_id, SessionStatus::Completed);
        }
        Err(e) => {
            state
                .jobs
                .fail(&job_id, format!("inferd request failed: {e}"));
            state
                .sessions
                .set_status(&job.session_id, SessionStatus::Error);
        }
    }
}
