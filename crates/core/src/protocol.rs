//! OpenAI-compatible protocol types.
//!
//! Moved from `crates/protocol` into core so all crates can use them
//! without adding a protocol dependency.

use serde::{Deserialize, Serialize};

use crate::backend::{CompletionRequest, CompletionResponse, Usage};

// ── Health ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub backend: String,
    pub template: Option<serde_json::Value>,
}

// ── Jobs ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferJobsResponse {
    pub status: String,
    pub backend: String,
    pub active_jobs: usize,
    pub uptime_secs: u64,
    pub features: Vec<String>,
}

// ── Bake ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeRequest {
    pub session_id: String,
    pub system: String,
    pub few_shots: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakeResponse {
    pub session_id: String,
    pub baked_shots: usize,
}

// ── OpenAI Completion ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompletionRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserve_state: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl OpenAiCompletionRequest {
    /// Convert to the internal `CompletionRequest`.
    pub fn into_engine(self) -> CompletionRequest {
        CompletionRequest {
            system: self.system.unwrap_or_default(),
            prompt: self.prompt,
            grammar: self.grammar,
            temperature: self.temperature.unwrap_or(0.7),
            max_tokens: self.max_tokens.unwrap_or(512),
            prefill: self.prefill,
            session: self.session,
            bnf_mask: None,
            on_token: None,
            preserve_state: self.preserve_state.unwrap_or(false),
        }
    }
}

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
    pub fn from_engine(id: String, model: String, resp: &CompletionResponse) -> Self {
        Self {
            id,
            object: "text_completion".into(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChoice {
    pub text: String,
    pub index: usize,
    pub logprobs: Option<serde_json::Value>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiErrorBody {
    pub error: OpenAiErrorDetail,
}

impl OpenAiErrorBody {
    pub fn new(message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            error: OpenAiErrorDetail {
                message: message.into(),
                type_: code.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiStreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiStreamChoice>,
}

impl OpenAiStreamChunk {
    pub fn token(id: String, model: String, token: &str) -> Self {
        Self {
            id,
            object: "text_completion.chunk".into(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            model,
            choices: vec![OpenAiStreamChoice {
                text: token.into(),
                index: 0,
                finish_reason: None,
            }],
        }
    }

    pub fn stop(id: String, model: String) -> Self {
        Self {
            id,
            object: "text_completion.chunk".into(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            model,
            choices: vec![OpenAiStreamChoice {
                text: "".into(),
                index: 0,
                finish_reason: Some("stop".into()),
            }],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiStreamChoice {
    pub text: String,
    pub index: usize,
    pub finish_reason: Option<String>,
}


