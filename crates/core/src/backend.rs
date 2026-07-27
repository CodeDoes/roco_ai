//! The ModelBackend trait — the single abstraction over all inference backends.
//!
//! Moved from `crates/engine` so that `crates/core` can define the trait
//! without depending on the full engine implementation.

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::error::RoCoResult;

/// A completion request sent to any backend.
#[derive(Debug, Clone, Default)]
pub struct CompletionRequest {
    pub system: String,
    pub prompt: String,
    pub grammar: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
    pub prefill: Option<String>,
    pub session: Option<String>,
    pub bnf_mask: Option<Box<dyn std::any::Any + Send + Sync>>,
    pub on_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
    pub preserve_state: bool,
}

impl CompletionRequest {
    pub fn new(system: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            prompt: prompt.into(),
            temperature: 0.7,
            max_tokens: 512,
            ..Default::default()
        }
    }

    pub fn with_grammar(mut self, grammar: impl Into<String>) -> Self {
        self.grammar = Some(grammar.into());
        self
    }

    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }

    pub fn with_max_tokens(mut self, n: usize) -> Self {
        self.max_tokens = n;
        self
    }

    pub fn with_session(mut self, s: impl Into<String>) -> Self {
        self.session = Some(s.into());
        self
    }
}

/// Token usage statistics.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

impl Usage {
    pub fn total(&self) -> usize {
        self.prompt_tokens + self.completion_tokens
    }
}

/// A completion response from any backend.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
    pub usage: Usage,
}

/// The trait implemented by all inference backends (local GPU, remote API, mock).
#[async_trait]
pub trait ModelBackend: Send + Sync {
    /// Generate text from a prompt.
    async fn complete(&self, req: CompletionRequest) -> RoCoResult<CompletionResponse>;

    /// Feed an EOS token into a session to reset state without losing context.
    async fn feed_eos(&self, session_id: Option<String>) -> RoCoResult<()>;

    /// Return the model name / identifier.
    fn name(&self) -> &str;

    /// Downcast helper for optional `StateTuning` support.
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// Return vocabulary bytes for BNF mask construction, if available.
    fn vocab_bytes(&self) -> Option<Vec<Vec<u8>>> {
        None
    }

    /// Blend two session states together.
    async fn blend_states(
        &self,
        session_a: &str,
        session_b: &str,
        alpha: f32,
        output_session: &str,
    ) -> RoCoResult<()> {
        let _ = (session_a, session_b, alpha, output_session);
        Err(crate::error::RoCoError::Backend(
            "blend_states not supported".into(),
        ))
    }
}

/// Optional: state-tuning for RNN-based backends (RWKV).
///
/// This is a separate trait so that transformer backends — or mocks — are
/// not forced to provide a meaningless `bake_state` implementation.
#[async_trait]
pub trait StateTuning: ModelBackend {
    /// Bake few-shot examples into a named session to prime the recurrent
    /// state, bypassing replay.
    async fn bake_state(
        &self,
        session_id: &str,
        system: &str,
        few_shots: &[(&str, &str)],
    ) -> RoCoResult<String>;
}

/// A mock backend for testing.
#[derive(Debug, Default)]
pub struct MockBackend {
    pub responses: std::sync::Arc<parking_lot::Mutex<Vec<String>>>,
}

#[async_trait]
impl ModelBackend for MockBackend {
    async fn complete(&self, req: CompletionRequest) -> RoCoResult<CompletionResponse> {
        let mut responses = self.responses.lock();
        let text = if responses.is_empty() {
            format!("mock response to: {}", req.prompt.chars().take(40).collect::<String>())
        } else {
            responses.remove(0)
        };
        Ok(CompletionResponse {
            text,
            usage: Usage::default(),
        })
    }

    async fn feed_eos(&self, _session_id: Option<String>) -> RoCoResult<()> {
        Ok(())
    }

    fn name(&self) -> &str {
        "mock"
    }
}

#[async_trait]
impl StateTuning for MockBackend {
    async fn bake_state(
        &self,
        session_id: &str,
        _system: &str,
        _few_shots: &[(&str, &str)],
    ) -> RoCoResult<String> {
        Err(crate::error::RoCoError::Backend(
            "bake_state not supported by MockBackend".into(),
        ))
    }
}



