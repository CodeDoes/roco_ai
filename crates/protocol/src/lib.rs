//! Shared protocol types for RoCo HTTP API.
//!
//! Provides wire-format request and response types used by the server
//! (inference daemon) and gateway (remote proxy). Both crates share
//! these definitions so serialization and deserialization stay in sync
//! — no type drift between endpoints.
//!
//! # Wire format
//!
//! The server speaks an OpenAI-compatible `/v1/completions` endpoint.
//! `OpenAiCompletionRequest` and `OpenAiCompletionResponse` map directly
//! to the OpenAI HTTP body shape, with RoCo-specific extensions
//! (`thinking`, `grammar`, `prefill`, `session`, `preserve_state`).

use roco_engine::CompletionRequest;
use serde::{Deserialize, Serialize};

// ── Request types ──────────────────────────────────────────────────────────

/// OpenAI-compatible completion request, with RoCo-specific extensions.
///
/// This is the **wire format**. Convert to/from [`CompletionRequest`] (the
/// engine type) via [`From`] impls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompletionRequest {
    #[serde(default)]
    pub model: Option<String>,
    pub prompt: String,
    pub system: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub stream: Option<bool>,
    /// Enable think-trace extraction (RoCo extension).
    #[serde(default)]
    pub thinking: Option<bool>,
    /// Grammar name for constrained decoding (RoCo extension).
    pub grammar: Option<String>,
    /// Prefill text to inject after "Assistant:" (RoCo extension).
    pub prefill: Option<String>,
    /// Named recurrent-state session to load/save (RoCo extension).
    #[serde(default)]
    pub session: Option<String>,
    /// Preserve recurrent state after this completion (RoCo extension).
    #[serde(default)]
    pub preserve_state: Option<bool>,
}

impl OpenAiCompletionRequest {
    /// Convert to the engine's `CompletionRequest`, consuming self.
    pub fn into_engine(self) -> CompletionRequest {
        CompletionRequest {
            system: self.system.unwrap_or_default(),
            prompt: self.prompt,
            prefill: self.prefill,
            grammar: self.grammar,
            temperature: self.temperature.unwrap_or(0.2),
            max_tokens: self.max_tokens.unwrap_or(512),
            thinking: self.thinking.unwrap_or(false),
            session: self.session,
            preserve_state: self.preserve_state.unwrap_or(false),
            ..Default::default()
        }
    }

    /// Build from an engine `CompletionRequest`.
    pub fn from_engine(req: &CompletionRequest) -> Self {
        Self {
            model: None,
            prompt: req.prompt.clone(),
            system: Some(req.system.clone()).filter(|s| !s.is_empty()),
            temperature: Some(req.temperature),
            max_tokens: Some(req.max_tokens),
            stream: None,
            thinking: Some(req.thinking),
            grammar: req.grammar.clone(),
            prefill: req.prefill.clone(),
            session: req.session.clone(),
            preserve_state: Some(req.preserve_state),
        }
    }
}

// ── Response types (non-streaming) ────────────────────────────────────────

/// OpenAI-compatible non-streaming completion response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiChoice>,
    pub usage: OpenAiUsage,
}

impl OpenAiCompletionResponse {
    /// Build a response from an engine result.
    pub fn from_engine(id: String, model: String, resp: &roco_engine::CompletionResponse) -> Self {
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id,
            object: "text_completion".into(),
            created,
            model,
            choices: vec![OpenAiChoice {
                text: resp.text.clone(),
                index: 0,
                logprobs: None,
                finish_reason: Some("stop".into()),
            }],
            usage: OpenAiUsage {
                prompt_tokens: resp.usage.prompt_tokens,
                completion_tokens: resp.usage.completion_tokens,
                total_tokens: resp.usage.total(),
            },
        }
    }
}

/// A single completion choice (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChoice {
    pub text: String,
    pub index: usize,
    pub logprobs: Option<serde_json::Value>,
    pub finish_reason: Option<String>,
}

/// Token usage summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

// ── Response types (streaming) ────────────────────────────────────────────

/// OpenAI-compatible streaming chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiStreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiStreamChoice>,
}

impl OpenAiStreamChunk {
    /// Build a token chunk.
    pub fn token(id: String, model: String, text: &str) -> Self {
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id,
            object: "text_completion".into(),
            created,
            model,
            choices: vec![OpenAiStreamChoice {
                text: text.to_string(),
                index: 0,
                finish_reason: None,
            }],
        }
    }

    /// Build a final-stop chunk.
    pub fn stop(id: String, model: String) -> Self {
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id,
            object: "text_completion".into(),
            created,
            model,
            choices: vec![OpenAiStreamChoice {
                text: String::new(),
                index: 0,
                finish_reason: Some("stop".into()),
            }],
        }
    }
}

/// A single streaming choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiStreamChoice {
    pub text: String,
    pub index: usize,
    pub finish_reason: Option<String>,
}

// ── Error response ────────────────────────────────────────────────────────

/// Standard error body returned by the server/gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiErrorBody {
    pub error: OpenAiErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub param: Option<serde_json::Value>,
    pub code: Option<String>,
}

impl OpenAiErrorBody {
    pub fn new(message: impl Into<String>, error_type: impl Into<String>) -> Self {
        Self {
            error: OpenAiErrorDetail {
                message: message.into(),
                error_type: error_type.into(),
                param: None,
                code: None,
            },
        }
    }
}

// ── Health ────────────────────────────────────────────────────────────────

/// Standard health-check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub backend: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_minimal_deserialize() {
        let json = r#"{"prompt": "Hello world"}"#;
        let req: OpenAiCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "Hello world");
        assert!(req.system.is_none());
        assert!(req.temperature.is_none());
        assert!(req.stream.is_none());
        assert!(req.session.is_none());
    }

    #[test]
    fn request_full_deserialize() {
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
    fn response_serialization() {
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
    fn stream_chunk_serialization() {
        let chunk = OpenAiStreamChunk::token("stream-1".into(), "rwkv-7".into(), "partial");
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("stream-1"));
        assert!(json.contains("partial"));
        assert!(json.contains("\"finish_reason\":null"));
    }

    #[test]
    fn stream_stop_chunk() {
        let chunk = OpenAiStreamChunk::stop("s-1".into(), "rwkv-7".into());
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("\"finish_reason\":\"stop\""));
    }

    #[test]
    fn error_body_serialization() {
        let err = OpenAiErrorBody::new("something broke", "backend_error");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("something broke"));
        assert!(json.contains("backend_error"));
    }

    #[test]
    fn engine_round_trip() {
        let engine_req = CompletionRequest::new("system", "hello");
        let wire = OpenAiCompletionRequest::from_engine(&engine_req);
        assert_eq!(wire.prompt, "hello");
        assert_eq!(wire.system.as_deref(), Some("system"));
        let back = wire.into_engine();
        assert_eq!(back.system, "system");
        assert_eq!(back.prompt, "hello");
    }

    #[test]
    fn engine_round_trip_empty_system() {
        let engine_req = CompletionRequest::new("", "hello");
        let wire = OpenAiCompletionRequest::from_engine(&engine_req);
        assert_eq!(wire.prompt, "hello");
        // Empty system should become None on the wire
        assert!(wire.system.is_none());
        let back = wire.into_engine();
        assert_eq!(back.system, "");
        assert_eq!(back.prompt, "hello");
    }
}
