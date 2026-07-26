//! Error recovery: retry logic, fallback strategies, and graceful degradation
//! for inference and tool execution failures.
//!
//! Provides retry wrappers around the [`ModelBackend`] trait so that transient
//! failures (GPU timeouts, OOM, parse errors) are handled automatically.

use std::time::Duration;

use roco_engine::{CompletionRequest, CompletionResponse, ModelBackend};

/// How to retry when inference fails.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (0 = no retry).
    pub max_retries: u32,
    /// Base delay between retries (doubled each attempt).
    pub base_delay: Duration,
    /// Whether to fall back to non-grammar inference if grammar-constrained
    /// generation fails with a parse error.
    pub fallback_on_grammar_error: bool,
    /// Whether to shorten `max_tokens` on timeout/truncation errors.
    pub shorten_on_truncation: bool,
    /// How many tokens to subtract on each truncation retry.
    pub truncation_step: usize,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            base_delay: Duration::from_millis(500),
            fallback_on_grammar_error: true,
            shorten_on_truncation: true,
            truncation_step: 64,
        }
    }
}

/// Errors that can occur during inference.
#[derive(Debug, Clone)]
pub enum InferenceError {
    /// The grammar-constrained generation failed (e.g., grammar stuck).
    GrammarError(String),
    /// The model returned a timeout.
    Timeout,
    /// The output was truncated unexpectedly.
    Truncated {
        actual_tokens: usize,
        max_tokens: usize,
    },
    /// A tool call failed to parse.
    ToolCallParseError(String),
    /// A tool execution failed.
    ToolExecutionError(String),
    /// A general inference error.
    BackendError(String),
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceError::GrammarError(msg) => write!(f, "grammar error: {msg}"),
            InferenceError::Timeout => write!(f, "inference timeout"),
            InferenceError::Truncated {
                actual_tokens,
                max_tokens,
            } => {
                write!(f, "truncated at {actual_tokens}/{max_tokens} tokens")
            }
            InferenceError::ToolCallParseError(msg) => write!(f, "tool call parse error: {msg}"),
            InferenceError::ToolExecutionError(msg) => write!(f, "tool execution error: {msg}"),
            InferenceError::BackendError(msg) => write!(f, "backend error: {msg}"),
        }
    }
}

impl std::error::Error for InferenceError {}

impl From<anyhow::Error> for InferenceError {
    fn from(e: anyhow::Error) -> Self {
        InferenceError::BackendError(e.to_string())
    }
}

/// Run a completion with retry and fallback logic.
///
/// 1. Attempt the completion with the given grammar.
/// 2. If it fails with a grammar error and `fallback_on_grammar_error` is set,
///    retry without the grammar.
/// 3. If it fails with a timeout or truncation, retry with reduced `max_tokens`.
/// 4. Retry up to `config.max_retries` times with exponential backoff.
pub async fn complete_with_retry(
    backend: &dyn ModelBackend,
    mut req: CompletionRequest,
    config: &RetryConfig,
) -> Result<CompletionResponse, InferenceError> {
    let mut retries = 0u32;
    let mut last_error: Option<InferenceError> = None;

    loop {
        let had_grammar = req.grammar.is_some();

        match backend.complete(req.clone()).await {
            Ok(resp) => {
                // Check for truncation
                if config.shorten_on_truncation
                    && resp.usage.completion_tokens >= req.max_tokens
                    && req.max_tokens > config.truncation_step
                    && retries < config.max_retries
                {
                    req.max_tokens = req.max_tokens.saturating_sub(config.truncation_step);
                    retries += 1;
                    tokio::time::sleep(config.base_delay * (1u32 << retries.min(10))).await;
                    continue;
                }
                return Ok(resp);
            }
            Err(e) => {
                let err_str = e.to_string();
                last_error.replace(InferenceError::BackendError(err_str.clone()));

                if retries >= config.max_retries {
                    break;
                }

                // Grammar fallback: retry without grammar
                if config.fallback_on_grammar_error && had_grammar {
                    let is_grammar_error = err_str.contains("grammar")
                        || err_str.contains("Grammar")
                        || err_str.contains("BNF")
                        || err_str.contains("bnf")
                        || err_str.contains("schoolmarm");
                    if is_grammar_error {
                        req.grammar = None;
                        retries += 1;
                        tokio::time::sleep(config.base_delay * (1u32 << retries.min(10))).await;
                        continue;
                    }
                }

                retries += 1;
                tokio::time::sleep(config.base_delay * (1u32 << retries.min(10))).await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| InferenceError::BackendError("max retries exhausted".into())))
}

/// Check if a response looks like it was truncated mid-thought or mid-tool-call.
pub fn is_truncated_response(text: &str) -> bool {
    // Unclosed tags suggest truncation
    let open_tags = ["<think>", "<tool_call>", "<tool_result>"];
    let close_tags = ["</think>", "</tool_call>", "</tool_result>"];

    for (open, close) in open_tags.iter().zip(close_tags.iter()) {
        let open_count = text.matches(open).count();
        let close_count = text.matches(close).count();
        if open_count > close_count {
            return true;
        }
    }
    false
}

/// Extract a useful error message from a raw inference error.
pub fn describe_error(err: &InferenceError) -> String {
    match err {
        InferenceError::GrammarError(msg) => {
            format!("Grammar constraint failed: {msg}. The grammar may be too restrictive or the model is stuck.")
        }
        InferenceError::Timeout => {
            "Inference timed out. Try reducing max_tokens or using a simpler query.".into()
        }
        InferenceError::Truncated {
            actual_tokens,
            max_tokens,
        } => {
            format!("Response truncated at {actual_tokens}/{max_tokens} tokens.")
        }
        InferenceError::ToolCallParseError(msg) => {
            format!("Could not parse tool call: {msg}")
        }
        InferenceError::ToolExecutionError(msg) => {
            format!("Tool execution failed: {msg}")
        }
        InferenceError::BackendError(msg) => {
            format!("Backend error: {msg}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roco_engine::MockBackend;

    #[test]
    fn truncated_response_detection_open_think() {
        assert!(is_truncated_response("Some text <think>unclosed"));
        assert!(!is_truncated_response("Some text <think>closed</think>"));
    }

    #[test]
    fn truncated_response_detection_open_tool_call() {
        assert!(is_truncated_response("Text <tool_call>{\"name\":\"x\"}"));
        assert!(!is_truncated_response("Text <tool_call>{}</tool_call>"));
    }

    #[test]
    fn truncated_response_no_tags_is_not_truncated() {
        assert!(!is_truncated_response("Just regular text."));
    }

    #[test]
    fn retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 2);
        assert!(config.fallback_on_grammar_error);
    }

    #[test]
    fn describe_error_produces_human_readable_message() {
        let err = InferenceError::Timeout;
        let msg = describe_error(&err);
        let lower = msg.to_lowercase();
        assert!(
            lower.contains("timed") || lower.contains("timeout"),
            "message should mention timeout: {msg}"
        );
    }

    #[tokio::test]
    async fn complete_with_retry_succeeds_on_first_try() {
        let backend = MockBackend::new("mock", 0);
        let req = CompletionRequest::new("", "hello");
        let config = RetryConfig::default();
        let result = complete_with_retry(&backend, req, &config).await;
        assert!(
            result.is_ok(),
            "should succeed on first try: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn complete_with_retry_retries_on_failure() {
        let backend = MockBackend::new("mock", 3);
        let req = CompletionRequest::new("", "hello");
        let config = RetryConfig {
            max_retries: 5,
            ..Default::default()
        };
        let result = complete_with_retry(&backend, req, &config).await;
        assert!(
            result.is_ok(),
            "should succeed after retries: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn complete_with_retry_exhausts_retries() {
        let backend = MockBackend::new("mock", 10);
        let req = CompletionRequest::new("", "hello");
        let config = RetryConfig {
            max_retries: 2,
            ..Default::default()
        };
        let result = complete_with_retry(&backend, req, &config).await;
        assert!(result.is_err(), "should fail after exhausting retries");
    }
}
