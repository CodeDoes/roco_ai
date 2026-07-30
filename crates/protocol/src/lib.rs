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
//! (`grammar`, `prefill`, `init_state`, `state_slot`).

use roco_engine::CompletionRequest;
use serde::{Deserialize, Serialize};

pub mod chat_common;
pub mod message;

pub use chat_common::*;
pub use message::*;

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
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub stream: Option<bool>,
    /// Grammar name for constrained decoding (RoCo extension).
    pub grammar: Option<String>,
    /// Prefill text to inject after the prompt (RoCo extension).
    pub prefill: Option<String>,
    /// Load state from this cache slot before processing (RoCo extension).
    #[serde(default)]
    pub init_state: Option<String>,
    /// Save resulting state to this cache slot (RoCo extension).
    #[serde(default)]
    pub state_slot: Option<String>,
    /// DEPRECATED: use `init_state` and `state_slot` instead. Legacy session ID.
    #[serde(default)]
    pub session: Option<String>,
    /// Deterministic seed for reproducible sampling (RoCo extension).
    /// Same seed + same prompt + same temperature = same output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

impl OpenAiCompletionRequest {
    /// Convert to the engine's `CompletionRequest`, consuming self.
    pub fn into_engine(self) -> CompletionRequest {
        // If session is set, use it as both init and state slot for backward compat
        let init_state = self.init_state.or_else(|| self.session.clone());
        let state_slot = self.state_slot.or_else(|| self.session.clone());
        
        CompletionRequest {
            prompt: self.prompt,
            prefill: self.prefill,
            grammar: self.grammar,
            temperature: self.temperature.unwrap_or(0.2),
            max_tokens: self.max_tokens.unwrap_or(512),
            init_state,
            state_slot,
            seed: self.seed,
            ..Default::default()
        }
    }

    /// Build from an engine `CompletionRequest`.
    pub fn from_engine(req: &CompletionRequest) -> Self {
        Self {
            model: None,
            prompt: req.prompt.clone(),
            temperature: Some(req.temperature),
            max_tokens: Some(req.max_tokens),
            stream: None,
            grammar: req.grammar.clone(),
            prefill: req.prefill.clone(),
            init_state: req.init_state.clone(),
            state_slot: req.state_slot.clone(),
            session: None, // DEPRECATED: always None unless converting from legacy wire format
            seed: req.seed,
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
    /// Per-token trace metadata (only populated when record_trace=true).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<roco_engine::TokenTrace>,
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
            trace: resp.trace.clone(),
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

// ── Bake ──────────────────────────────────────────────────────────────────

/// Request to bake a set of few-shot example pairs into a named session state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeRequest {
    pub session_id: String,
    #[serde(default)]
    pub system: String,
    pub few_shots: Vec<(String, String)>,
}

/// Response returned after successfully baking few-shot state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeResponse {
    pub session_id: String,
    pub baked_shots: usize,
}

// ── Health ────────────────────────────────────────────────────────────────

/// Standard health-check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<serde_json::Value>,
}

/// Detailed status and active job metrics for the inference daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferJobsResponse {
    pub status: String,
    pub backend: String,
    pub active_jobs: usize,
    pub uptime_secs: u64,
    pub features: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_with_seed() {
        let json = r#"{"prompt": "hello", "seed": 42, "temperature": 0}"#;
        let req: OpenAiCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.seed, Some(42));
        let engine = req.into_engine();
        assert_eq!(engine.seed, Some(42));
    }

    #[test]
    fn request_without_seed_defaults_to_none() {
        let json = r#"{"prompt": "hello"}"#;
        let req: OpenAiCompletionRequest = serde_json::from_str(json).unwrap();
        assert!(req.seed.is_none());
        let engine = req.into_engine();
        assert!(engine.seed.is_none());
    }

    #[test]
    fn seed_propagates_through_from_engine() {
        let mut engine_req = CompletionRequest::new("prompt");
        engine_req.seed = Some(999);
        let wire = OpenAiCompletionRequest::from_engine(&engine_req);
        assert_eq!(wire.seed, Some(999));
        let back = wire.into_engine();
        assert_eq!(back.seed, Some(999));
    }

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
            trace: Vec::new(),
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
        let engine_req = CompletionRequest::new("hello");
        let wire = OpenAiCompletionRequest::from_engine(&engine_req);
        assert_eq!(wire.prompt, "hello");
        assert_eq!(wire.system.as_deref(), Some("system"));
        let back = wire.into_engine();
        assert_eq!(back.system, "system");
        assert_eq!(back.prompt, "hello");
    }

    #[test]
    fn engine_round_trip_empty_system() {
        let engine_req = CompletionRequest::new("hello");
        let wire = OpenAiCompletionRequest::from_engine(&engine_req);
        assert_eq!(wire.prompt, "hello");
        // Empty system should become None on the wire
        assert!(wire.system.is_none());
        let back = wire.into_engine();
        assert_eq!(back.system, "");
        assert_eq!(back.prompt, "hello");
    }
}
