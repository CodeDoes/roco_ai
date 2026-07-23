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
use roco_engine::{CompletionRequest, CompletionResponse, ModelBackend};
use roco_protocol::{
    HealthResponse, OpenAiCompletionRequest, OpenAiCompletionResponse, OpenAiErrorBody,
    OpenAiStreamChunk,
};

use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::info;

// Struct to hold Server State
#[derive(Clone)]
pub struct AppState {
    pub backend: Arc<dyn ModelBackend>,
}

pub fn create_router(backend: Arc<dyn ModelBackend>) -> Router {
    let state = AppState { backend };
    Router::new()
        .route("/health", get(handle_health))
        .route("/vocab", get(handle_vocab))
        .route("/complete", post(handle_complete))
        .route("/v1/completions", post(handle_openai_completion))
        .with_state(state)
}

async fn handle_health(State(state): State<AppState>) -> impl IntoResponse {
    let resp = HealthResponse {
        status: "ok".into(),
        backend: state.backend.name().to_string(),
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
    Json(req): Json<CompletionRequest>,
) -> Result<Json<CompletionResponse>, String> {
    info!("Handling direct complete request");
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
    info!(
        "Handling OpenAI completion request for prompt (len={})",
        req.prompt.len()
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
        let engine_req = req.into_engine();
        match backend.complete(engine_req).await {
            Ok(resp) => {
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
            "preserve_state": true
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
