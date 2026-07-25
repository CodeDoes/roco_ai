//! Core types for the inference pipeline.
//!
//! Defines [`CompletionRequest`], [`CompletionResponse`], [`EngineError`],
//! [`TokenUsage`], and [`TokenCounter`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Callback invoked for each token emitted during streaming generation.
pub type OnToken = Option<Box<dyn Fn(&str) + Send + Sync>>;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("backend failure: {0}")]
    Backend(String),
    #[error("empty completion returned by backend")]
    EmptyResponse,
    #[error("context budget exceeded: used {used} of {max} tokens")]
    BudgetExceeded { used: usize, max: usize },
    #[error("completion timed out after {ms} ms")]
    TimedOut { ms: u64 },
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
#[derive(Serialize, Deserialize)]
pub struct CompletionRequest {
    #[serde(default)]
    pub system: String,
    pub prompt: String,
    /// Text appended after "Assistant: " so the model sees it as its own
    /// completed output (e.g. pre-filled think blocks, assistant role-play).
    pub prefill: Option<String>,
    pub output_schema: Option<String>,
    pub grammar: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    pub top_a: Option<f32>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default)]
    pub estimated_prompt_tokens: usize,
    #[serde(default)]
    pub thinking: bool,
    #[serde(default)]
    pub preserve_state: bool,
    #[serde(skip)]
    pub on_token: OnToken,
    pub session: Option<String>,
    /// Wall-clock deadline for the entire completion (including prompt
    /// processing and all generated tokens). Specified in milliseconds.
    /// 0 = no deadline (default). When exceeded, the backend cancels
    /// the in-flight generation and returns `EngineError::TimedOut`.
    #[serde(default)]
    pub deadline_ms: u64,
    /// Opaque grammar constraint. Created by the application layer using
    /// `roco-bnf-engine::BnfEngine` to avoid pulling kbnf types into
    /// downstream crates that depend on `web-rwkv`.
    #[serde(skip)]
    pub bnf_mask: Option<Box<dyn BnfMask>>,
}

fn default_temperature() -> f32 {
    0.2
}

fn default_max_tokens() -> usize {
    512
}

impl Clone for CompletionRequest {
    fn clone(&self) -> Self {
        Self {
            system: self.system.clone(),
            prompt: self.prompt.clone(),
            prefill: self.prefill.clone(),
            output_schema: self.output_schema.clone(),
            grammar: self.grammar.clone(),
            temperature: self.temperature,
            top_a: self.top_a,
            max_tokens: self.max_tokens,
            estimated_prompt_tokens: self.estimated_prompt_tokens,
            thinking: self.thinking,
            preserve_state: self.preserve_state,
            session: self.session.clone(),
            deadline_ms: self.deadline_ms,
            on_token: None,
            bnf_mask: None,
        }
    }
}

impl std::fmt::Debug for CompletionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompletionRequest")
            .field("system", &self.system)
            .field("prompt", &self.prompt)
            .field("prefill", &self.prefill)
            .field("output_schema", &self.output_schema)
            .field("grammar", &self.grammar)
            .field("temperature", &self.temperature)
            .field("top_a", &self.top_a)
            .field("max_tokens", &self.max_tokens)
            .field("estimated_prompt_tokens", &self.estimated_prompt_tokens)
            .field("thinking", &self.thinking)
            .field("preserve_state", &self.preserve_state)
            .field("session", &self.session)
            .field("deadline_ms", &self.deadline_ms)
            .field("on_token", &self.on_token.as_ref().map(|_| "<callback>"))
            .field("bnf_mask", &self.bnf_mask.as_ref().map(|_| "<BnfMask>"))
            .finish()
    }
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            system: String::new(),
            prompt: String::new(),
            prefill: None,
            output_schema: None,
            grammar: None,
            temperature: 0.2,
            top_a: None,
            max_tokens: 512,
            estimated_prompt_tokens: 0,
            thinking: false,
            preserve_state: false,
            on_token: None,
            session: None,
            deadline_ms: 0,
            bnf_mask: None,
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
    pub parsed: Option<serde_json::Value>,
    pub think_trace: Option<String>,
}

/// Opaque BNF/logit-masking callback for grammar-constrained generation.
///
/// This trait is deliberately minimal — no references to kbnf, schoolmarm,
/// or any other grammar engine. The inference loop calls [`mask`] on each
/// step to zero out disallowed logits, then calls [`accept`] after sampling
/// a token to advance the grammar state.
///
/// Implementations live outside this crate (e.g. in `roco-bnf-engine`)
/// and are passed in as `Box<dyn BnfMask>` to avoid pulling grammar-engine
/// types into the inference compilation unit.
pub trait BnfMask: Send {
    /// Modify `logits` in place, setting disallowed tokens to
    /// `f32::NEG_INFINITY`.
    fn mask(&mut self, logits: &mut [f32]);
    /// Notify the grammar that `token_id` was just sampled.
    /// Returns `false` if the grammar is finished (no more tokens expected).
    fn accept(&mut self, token_id: u32) -> bool;
}

/// Cheap heuristic tokenizer (~4 chars/token for English).
pub struct TokenCounter;

impl TokenCounter {
    pub fn estimate(text: &str) -> usize {
        (text.chars().count() / 4).max(1)
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

    #[test]
    fn completion_request_new_sets_fields() {
        let req = CompletionRequest::new("system prompt", "user message");
        assert_eq!(req.system, "system prompt");
        assert_eq!(req.prompt, "user message");
        assert_eq!(req.temperature, 0.2);
        assert_eq!(req.max_tokens, 512);
    }

    #[test]
    fn completion_request_deserialize_minimal() {
        let json = r#"{"system": "test", "prompt": "hello", "temperature": 0.5, "max_tokens": 10}"#;
        let req: CompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "hello");
        assert_eq!(req.temperature, 0.5);
        assert_eq!(req.max_tokens, 10);
        assert_eq!(req.system, "test");
    }
}
