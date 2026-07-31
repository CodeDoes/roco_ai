use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use roco_app::RoCoConfig;
use roco_engine::{CompletionRequest, CompletionResponse, ModelBackend};
use roco_protocol::{
    BakeRequest, BakeResponse, HealthResponse, InferJobsResponse, OpenAiCompletionRequest,
    OpenAiCompletionResponse, OpenAiErrorBody, OpenAiStreamChunk,
};

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::info;

// Struct to hold Server State
#[derive(Clone)]
pub struct AppState {
    pub backend: Arc<dyn ModelBackend>,
    pub active_jobs: Arc<AtomicUsize>,
    pub start_time: std::time::Instant,
}

struct JobGuard(Arc<AtomicUsize>);

impl JobGuard {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        JobGuard(counter)
    }
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Build a `BnfMask` from a grammar string + the backend's vocabulary.
///
/// This is the single site in the codebase that compiles a grammar string
/// into a `Box<dyn BnfMask>` at runtime. `roco-server` (linked into
/// `roco-inferd`) is the only binary that depends on `roco-bnf-engine`,
/// keeping kbnf types out of every other compilation unit (avoids E0275
/// when kbnf sits alongside `web-rwkv::TokioRuntime`).
///
/// Returns `Ok(Some(mask))` if a grammar was provided, `Ok(None)` if no
/// grammar was set, or `Err(msg)` if the grammar is invalid.
fn build_mask_or_error(
    grammar: &Option<String>,
    backend: &dyn ModelBackend,
) -> Result<Option<Box<dyn roco_engine::BnfMask>>, String> {
    let Some(grammar) = grammar else {
        return Ok(None);
    };
    if grammar.is_empty() {
        return Ok(None);
    }
    let Some(vocab) = backend.vocab_bytes() else {
        return Ok(None);
    };
    roco_engine::create_bnf_mask(grammar, &vocab)
        .map(Some)
        .map_err(|e| {
            format!(
                "Grammar '{}' is invalid: {e:?}",
                grammar.chars().take(80).collect::<String>()
            )
        })
}

pub fn create_router(backend: Arc<dyn ModelBackend>) -> Router {
    let state = AppState {
        backend,
        active_jobs: Arc::new(AtomicUsize::new(0)),
        start_time: std::time::Instant::now(),
    };
    Router::new()
        .route("/health", get(handle_health))
        .route("/jobs", get(handle_jobs))
        .route("/vocab", get(handle_vocab))
        .route("/complete", post(handle_complete))
        .route("/v1/completions", post(handle_openai_completion))
        .route("/bake", post(handle_bake))
        .route("/v1/bake", post(handle_bake))
        .with_state(state)
}

async fn handle_bake(
    State(state): State<AppState>,
    Json(req): Json<BakeRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<OpenAiErrorBody>)> {
    let _guard = JobGuard::new(state.active_jobs.clone());
    let start = std::time::Instant::now();
    info!(
        session_id = %req.session_id,
        few_shots_count = req.few_shots.len(),
        system_len = req.system.len(),
        "Request → POST /v1/bake (baking {} few-shot pairs into session '{}')",
        req.few_shots.len(),
        req.session_id
    );

    let shots_ref: Vec<(&str, &str)> = req
        .few_shots
        .iter()
        .map(|(u, a)| (u.as_str(), a.as_str()))
        .collect();

    let session_id = state
        .backend
        .bake_state(&req.session_id, &req.system, &shots_ref)
        .await
        .map_err(|e| {
            tracing::warn!(
                session_id = %req.session_id,
                latency_ms = start.elapsed().as_millis() as u64,
                error = %e,
                "State baking failed"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OpenAiErrorBody::new(
                    format!("Bake state failed: {e}"),
                    "internal_error",
                )),
            )
        })?;

    let latency_ms = start.elapsed().as_millis() as u64;
    info!(
        session_id = %session_id,
        baked_shots = req.few_shots.len(),
        latency_ms = latency_ms,
        "Response ← POST /v1/bake (HTTP 200 in {}ms for session '{}')",
        latency_ms,
        session_id
    );

    Ok(Json(BakeResponse {
        session_id,
        baked_shots: req.few_shots.len(),
    }))
}

async fn handle_health(State(state): State<AppState>) -> impl IntoResponse {
    let config = RoCoConfig::load();
    let resp = HealthResponse {
        status: "ok".into(),
        backend: state.backend.name().to_string(),
        template: Some(serde_json::json!({
            "type": config.template.r#type,
            "think": config.template.think,
            "state_tune": config.template.state_tune,
            "system_prompt": config.template.system_prompt,
            "context_state": config.template.context_state,
            "max_tokens": config.template.max_tokens,
        })),
    };
    Json(resp)
}

async fn handle_jobs(State(state): State<AppState>) -> impl IntoResponse {
    let resp = InferJobsResponse {
        status: "online".into(),
        backend: state.backend.name().to_string(),
        active_jobs: state.active_jobs.load(Ordering::SeqCst),
        uptime_secs: state.start_time.elapsed().as_secs(),
        features: vec![
            "bnf_grammar".into(),
            "session_baking".into(),
            "think_extraction".into(),
            "openai_compat".into(),
        ],
    };
    Json(resp)
}

/// Return the model vocabulary as base64-encoded per-token byte strings.
/// Used by remote clients to build BNF grammar masks locally (the mask
/// builder must run in the client's compilation unit, not the server's).
async fn handle_vocab(State(state): State<AppState>) -> impl IntoResponse {
    match state.backend.vocab_bytes() {
        Some(vocab) => {
            let b64: Vec<String> = vocab
                .iter()
                .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes))
                .collect();
            Json(serde_json::json!({ "vocab": b64 })).into_response()
        }
        None => (StatusCode::NOT_IMPLEMENTED, "vocab not available").into_response(),
    }
}

async fn handle_complete(
    State(state): State<AppState>,
    Json(mut req): Json<CompletionRequest>,
) -> Result<Json<CompletionResponse>, String> {
    let _guard = JobGuard::new(state.active_jobs.clone());
    info!("Handling direct complete request");

    if req.bnf_mask.is_none() {
        req.bnf_mask = build_mask_or_error(&req.grammar, state.backend.as_ref())?;
        if req.bnf_mask.is_some() {
            info!(
                "Built BnfMask from grammar ({} chars)",
                req.grammar.as_deref().map(|g| g.len()).unwrap_or(0)
            );
        }
    }

    let resp = state
        .backend
        .complete(req)
        .await
        .map_err(|e| format!("Backend error: {e}"))?;
    Ok(Json(resp))
}

async fn handle_openai_completion(
    State(state): State<AppState>,
    Json(req): Json<OpenAiCompletionRequest>,
) -> impl IntoResponse {
    let _guard = JobGuard::new(state.active_jobs.clone());
    let start = std::time::Instant::now();
    let session_str = req.session.as_deref().unwrap_or("<none>").to_string();
    let prompt_tail = req
        .prompt
        .lines()
        .last()
        .unwrap_or("")
        .chars()
        .take(60)
        .collect::<String>();

    info!(
        session = %session_str,
        prompt_len = req.prompt.len(),
        max_tokens = req.max_tokens.unwrap_or(512),
        temperature = req.temperature.unwrap_or(0.2),
        grammar = req.grammar.as_deref().unwrap_or("none"),
        "Request → POST /v1/completions (session: '{}', prompt: {} bytes, tail: '...{}')",
        session_str,
        req.prompt.len(),
        prompt_tail
    );

    let is_stream = req.stream.unwrap_or(false);
    let model_name = state.backend.name().to_string();
    let backend = state.backend.clone();

    if is_stream {
        let (tx, rx) = mpsc::channel::<Result<Event, std::convert::Infallible>>(100);
        let req_id = format!(
            "cmpl-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        );

        let tx_stop = tx.clone();
        tokio::spawn(async move {
            let req_id_clone = req_id.clone();
            let on_token = Box::new(move |token: &str| {
                let chunk =
                    OpenAiStreamChunk::token(req_id_clone.clone(), model_name.clone(), token);
                if let Ok(json_str) = serde_json::to_string(&chunk) {
                    let _ = tx.try_send(Ok(Event::default().data(json_str)));
                }
            });

            let engine_req = req.into_engine();
            let full_req = CompletionRequest {
                on_token: Some(on_token),
                ..engine_req
            };

            let _ = backend.complete(full_req).await;

            // Send closing choice
            let stop = OpenAiStreamChunk::stop(req_id.clone(), backend.name().to_string());
            if let Ok(json_str) = serde_json::to_string(&stop) {
                let _ = tx_stop.try_send(Ok(Event::default().data(json_str)));
            }
        });

        let stream = ReceiverStream::new(rx);
        Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response()
    } else {
        let mut engine_req = req.into_engine();

        if engine_req.bnf_mask.is_none() {
            engine_req.bnf_mask = build_mask_or_error(&engine_req.grammar, backend.as_ref())
                .ok()
                .flatten();
        }

        match backend.complete(engine_req).await {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                let snippet = resp
                    .text
                    .chars()
                    .take(80)
                    .collect::<String>()
                    .replace('\n', " ");
                info!(
                    session = %session_str,
                    latency_ms = latency_ms,
                    prompt_tokens = resp.usage.prompt_tokens,
                    completion_tokens = resp.usage.completion_tokens,
                    total_tokens = resp.usage.total(),
                    "Response ← POST /v1/completions (HTTP 200 in {}ms, {} prompt + {} completion tokens): '{}...'",
                    latency_ms,
                    resp.usage.prompt_tokens,
                    resp.usage.completion_tokens,
                    snippet
                );

                let req_id = format!(
                    "cmpl-{}",
                    uuid::Uuid::new_v4()
                        .to_string()
                        .chars()
                        .take(8)
                        .collect::<String>()
                );
                let out_resp = OpenAiCompletionResponse::from_engine(
                    req_id,
                    backend.name().to_string(),
                    &resp,
                );
                Json(out_resp).into_response()
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                tracing::warn!(
                    session = %session_str,
                    latency_ms = latency_ms,
                    error = %e,
                    "Response ← POST /v1/completions (HTTP 500 error in {}ms: {e})",
                    latency_ms
                );
                let err_body = OpenAiErrorBody::new(format!("Backend error: {e}"), "backend_error");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(err_body)).into_response()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roco_protocol::*;

    #[test]
    fn test_openai_request_deserialize_minimal() {
        let json = r#"{"prompt": "Hello world"}"#;
        let req: OpenAiCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "Hello world");
        assert!(req.system.is_none());
        assert!(req.temperature.is_none());
        assert!(req.max_tokens.is_none());
        assert!(req.stream.is_none());
        assert!(req.model.is_none());
        assert!(req.session.is_none());
    }

    #[test]
    fn test_openai_request_deserialize_full() {
        let json = r#"{
            "model": "rwkv-7",
            "prompt": "Once upon a time",
            "system": "You are a storyteller.",
            "temperature": 0.8,
            "max_tokens": 200,
            "stream": true,
            "thinking": true,
            "grammar": "story",
            "prefill": "In a land far away",
            "session": "story-session-1",
            "preserve_state": true,
            "seed": 42
        }"#;
        let req: OpenAiCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model.as_deref(), Some("rwkv-7"));
        assert_eq!(req.prompt, "Once upon a time");
        assert_eq!(req.system.as_deref(), Some("You are a storyteller."));
        assert!((req.temperature.unwrap() - 0.8).abs() < 1e-6);
        assert_eq!(req.max_tokens, Some(200));
        assert_eq!(req.stream, Some(true));
        assert_eq!(req.thinking, Some(true));
        assert_eq!(req.grammar.as_deref(), Some("story"));
        assert_eq!(req.prefill.as_deref(), Some("In a land far away"));
        assert_eq!(req.session.as_deref(), Some("story-session-1"));
        assert_eq!(req.preserve_state, Some(true));
        assert_eq!(req.seed, Some(42));

        // Verify seed propagates to engine request
        let engine_req = req.into_engine();
        assert_eq!(engine_req.seed, Some(42));
    }

    #[test]
    fn test_openai_response_serialization() {
        let resp = OpenAiCompletionResponse {
            id: "cmpl-abc123".into(),
            object: "text_completion".into(),
            created: 1700000000,
            model: "rwkv-7".into(),
            choices: vec![OpenAiChoice {
                text: "Hello world".into(),
                index: 0,
                logprobs: None,
                finish_reason: Some("stop".into()),
            }],
            usage: OpenAiUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            trace: Vec::new(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("cmpl-abc123"));
        assert!(json.contains("Hello world"));
    }

    #[test]
    fn test_openai_stream_chunk_serialization() {
        let chunk = OpenAiStreamChunk::token("stream-1".into(), "rwkv-7".into(), "partial");
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("stream-1"));
        assert!(json.contains("partial"));
    }

    #[test]
    fn test_openai_error_body() {
        let err = OpenAiErrorBody::new("backend failure", "backend_error");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("backend failure"));
        assert!(json.contains("backend_error"));
    }
}
