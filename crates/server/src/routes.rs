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
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::info;

// Struct to hold Server State
#[derive(Clone)]
pub struct AppState {
    pub backend: Arc<dyn ModelBackend>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiCompletionRequest {
    #[serde(default)]
    pub model: Option<String>,
    pub prompt: String,
    pub system: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub stream: Option<bool>,
    pub thinking: Option<bool>,
    pub grammar: Option<String>,
    pub prefill: Option<String>,
    /// Named recurrent-state session to load before, and (with
    /// `preserve_state`) save after, this completion. Enables state-tuning
    /// (e.g. baking few-shot examples into a session the model resumes from).
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub preserve_state: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiChoice>,
    pub usage: OpenAiUsage,
}

#[derive(Debug, Serialize)]
pub struct OpenAiChoice {
    pub text: String,
    pub index: usize,
    pub logprobs: Option<serde_json::Value>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Serialize)]
pub struct OpenAiStreamResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiStreamChoice>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiStreamChoice {
    pub text: String,
    pub index: usize,
    pub finish_reason: Option<String>,
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

async fn handle_health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "backend": "rwkv" }))
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
    let sys = req.system.clone().unwrap_or_default();
    let prompt = req.prompt.clone();
    let temp = req.temperature.unwrap_or(0.2);
    let max_tok = req.max_tokens.unwrap_or(512);
    let think = req.thinking.unwrap_or(false);
    let sess = req.session.clone();
    let preserve = req.preserve_state.unwrap_or(false);

    let is_stream = req.stream.unwrap_or(false);

    if is_stream {
        let (tx, rx) = mpsc::channel::<Result<Event, std::convert::Infallible>>(100);
        let backend = state.backend.clone();
        let model_name = backend.name().to_string();

        let req_id = format!(
            "cmpl-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        );
        let req_id_clone = req_id.clone();

        tokio::spawn(async move {
            let tx_clone = tx.clone();
            let on_token = Box::new(move |token: &str| {
                let stream_resp = OpenAiStreamResponse {
                    id: req_id_clone.clone(),
                    object: "text_completion".to_string(),
                    created: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    model: model_name.clone(),
                    choices: vec![OpenAiStreamChoice {
                        text: token.to_string(),
                        index: 0,
                        finish_reason: None,
                    }],
                };
                if let Ok(json_str) = serde_json::to_string(&stream_resp) {
                    let _ = tx_clone.try_send(Ok(Event::default().data(json_str)));
                }
            });

            let full_req = CompletionRequest {
                system: sys,
                prompt,
                temperature: temp,
                max_tokens: max_tok,
                thinking: think,
                grammar: req.grammar,
                prefill: req.prefill,
                session: sess.clone(),
                preserve_state: preserve,
                on_token: Some(on_token),
                ..Default::default()
            };

            let _ = backend.complete(full_req).await;

            // Send closing choice
            let stream_resp = OpenAiStreamResponse {
                id: req_id.clone(),
                object: "text_completion".to_string(),
                created: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                model: backend.name().to_string(),
                choices: vec![OpenAiStreamChoice {
                    text: "".to_string(),
                    index: 0,
                    finish_reason: Some("stop".to_string()),
                }],
            };
            if let Ok(json_str) = serde_json::to_string(&stream_resp) {
                let _ = tx.try_send(Ok(Event::default().data(json_str)));
            }
        });

        let stream = ReceiverStream::new(rx);
        Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response()
    } else {
        let full_req = CompletionRequest {
            system: sys,
            prompt,
            temperature: temp,
            max_tokens: max_tok,
            thinking: think,
            grammar: req.grammar,
            prefill: req.prefill,
            session: sess.clone(),
            preserve_state: preserve,
            ..Default::default()
        };

        match state.backend.complete(full_req).await {
            Ok(resp) => {
                let created = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let req_id = format!(
                    "cmpl-{}",
                    uuid::Uuid::new_v4()
                        .to_string()
                        .chars()
                        .take(8)
                        .collect::<String>()
                );
                let out_resp = OpenAiCompletionResponse {
                    id: req_id,
                    object: "text_completion".to_string(),
                    created,
                    model: state.backend.name().to_string(),
                    choices: vec![OpenAiChoice {
                        text: resp.text,
                        index: 0,
                        logprobs: None,
                        finish_reason: Some("stop".to_string()),
                    }],
                    usage: OpenAiUsage {
                        prompt_tokens: resp.usage.prompt_tokens,
                        completion_tokens: resp.usage.completion_tokens,
                        total_tokens: resp.usage.total(),
                    },
                };
                Json(out_resp).into_response()
            }
            Err(e) => {
                let err_json = serde_json::json!({
                    "error": {
                        "message": format!("Backend error: {e}"),
                        "type": "backend_error",
                        "param": null,
                        "code": null
                    }
                });
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(err_json),
                )
                    .into_response()
            }
        }
    }
}
