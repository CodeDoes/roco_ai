//! Core types for the inference pipeline.
//!
//! Defines [`CompletionRequest`], [`CompletionResponse`], [`EngineError`],
//! [`TokenUsage`], and [`TokenCounter`].

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
#[derive(Serialize, Deserialize)]
pub struct CompletionRequest {
    pub system: String,
    pub prompt: String,
    pub output_schema: Option<String>,
    pub grammar: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
    pub estimated_prompt_tokens: usize,
    pub thinking: bool,
    pub preserve_state: bool,
    #[serde(skip)]
    pub on_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
    pub session: Option<String>,
}

impl Clone for CompletionRequest {
    fn clone(&self) -> Self {
        Self {
            system: self.system.clone(),
            prompt: self.prompt.clone(),
            output_schema: self.output_schema.clone(),
            grammar: self.grammar.clone(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            estimated_prompt_tokens: self.estimated_prompt_tokens,
            thinking: self.thinking,
            preserve_state: self.preserve_state,
            session: self.session.clone(),
            on_token: None,
        }
    }
}

impl std::fmt::Debug for CompletionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompletionRequest")
            .field("system", &self.system)
            .field("prompt", &self.prompt)
            .field("output_schema", &self.output_schema)
            .field("grammar", &self.grammar)
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("estimated_prompt_tokens", &self.estimated_prompt_tokens)
            .field("thinking", &self.thinking)
            .field("preserve_state", &self.preserve_state)
            .field("session", &self.session)
            .field("on_token", &self.on_token.as_ref().map(|_| "<callback>"))
            .finish()
    }
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            system: String::new(),
            prompt: String::new(),
            output_schema: None,
            grammar: None,
            temperature: 0.2,
            max_tokens: 512,
            estimated_prompt_tokens: 0,
            thinking: false,
            preserve_state: false,
            on_token: None,
            session: None,
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
}
