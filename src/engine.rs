//! Inference backend seam for RoCo AI.
//!
//! This module defines the model-agnostic interface that the orchestration
//! layer (`agent`) depends on. A concrete backend (e.g. a 3B RWKV/SSM model
//! downloaded later) implements [`ModelBackend`]; until then, [`MockBackend`]
//! lets the orchestration layer be built and tested without a model.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("backend failure: {0}")]
    Backend(String),
    #[error("empty completion returned by backend")]
    EmptyResponse,
    #[error("context budget exceeded: used {used} of {max} tokens")]
    BudgetExceeded { used: usize, max: usize },
}

/// Token accounting returned by a backend.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

impl TokenUsage {
    pub fn total(&self) -> usize {
        self.prompt_tokens + self.completion_tokens
    }
}

/// A completion request to a model backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// System / instruction block (role, output schema, do_nothing). See §2.2.
    pub system: String,
    /// The task-specific prompt. The schema is placed first (§2.2A).
    pub prompt: String,
    /// Optional strict output schema hint used for constrained decoding (§2.2D).
    pub output_schema: Option<String>,
    /// Sampling temperature. 0.1–0.2 for deterministic tasks (§2.2F).
    pub temperature: f32,
    /// Hard cap on generated tokens. Default 512 (§2.2F).
    pub max_tokens: usize,
    /// Caller-supplied prompt token estimate (filled via [`TokenCounter`]).
    pub estimated_prompt_tokens: usize,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            system: String::new(),
            prompt: String::new(),
            output_schema: None,
            temperature: 0.2,
            max_tokens: 512,
            estimated_prompt_tokens: 0,
        }
    }
}

impl CompletionRequest {
    pub fn new(system: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            prompt: prompt.into(),
            ..Default::default()
        }
    }
}

/// A completion produced by a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    pub usage: TokenUsage,
    /// Parsed JSON when the output was valid JSON / constrained.
    pub parsed: Option<serde_json::Value>,
}

/// Cheap heuristic tokenizer used until a real BPE/tiktoken backend is wired in.
/// ~4 chars/token is a reasonable English approximation (§4.1 budgeting).
pub struct TokenCounter;

impl TokenCounter {
    pub fn estimate(text: &str) -> usize {
        (text.chars().count() / 4).max(1)
    }
}

use futures::future::BoxFuture;

/// The model inference seam. A downloaded 3B model implements this later.
pub trait ModelBackend: Send + Sync {
    fn name(&self) -> &str;
    /// Whether constrained decoding (§2.2D) is available.
    fn supports_constrained_decoding(&self) -> bool {
        false
    }
    fn complete(
        &self,
        req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>>;
}

/// Deterministic backend for tests / pre-model development.
/// Echoes a schema-shaped JSON object so the orchestration layer is exercisable
/// without a real model.
#[derive(Debug, Clone, Default)]
pub struct MockBackend {
    pub name: String,
    pub latency_ms: u64,
}

impl ModelBackend for MockBackend {
    fn name(&self) -> &str {
        &self.name
    }
    fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            if self.latency_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.latency_ms)).await;
            }
            let snippet: String = req.prompt.chars().take(48).collect();
            // Build valid JSON via serde_json so newlines/quotes are escaped properly.
            let text = serde_json::json!({ "result": format!("[{}] {}", self.name, snippet) })
                .to_string();
            let parsed = serde_json::from_str(&text).ok();
            Ok(CompletionResponse {
                text,
                usage: TokenUsage {
                    prompt_tokens: req.estimated_prompt_tokens,
                    completion_tokens: 16,
                },
                parsed,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_counter_is_monotonic_and_floored() {
        assert_eq!(TokenCounter::estimate(""), 1);
        assert!(TokenCounter::estimate("hello world this is a test") >= 1);
    }

    #[tokio::test]
    async fn mock_backend_returns_parseable_json() {
        let b = MockBackend::default();
        let resp = b.complete(CompletionRequest::new("sys", "do the thing")).await.unwrap();
        assert!(resp.parsed.is_some());
        assert!(resp.text.contains("mock") || resp.text.contains("result"));
    }
}
