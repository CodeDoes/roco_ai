//! The ModelBackend trait — the single abstraction over all inference backends.
//!
//! Moved from `crates/engine` so that `crates/core` can define the trait
//! without depending on the full engine implementation.

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::error::RoCoResult;

/// A completion request sent to any backend.
///
/// inferd receives raw text only — no System/User/Assistant formatting.
/// State is managed explicitly via init_state (load from cache) and
/// state_slot (save to cache). None = start blank / don't cache.
#[derive(Debug, Clone, Default)]
pub struct CompletionRequest {
    pub prompt: String,
    pub grammar: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
    pub prefill: Option<String>,
    /// Load state from this cache slot before processing. None = start blank.
    pub init_state: Option<String>,
    /// Save resulting state to this cache slot. None = don't cache.
    pub state_slot: Option<String>,
    pub bnf_mask: Option<Box<dyn std::any::Any + Send + Sync>>,
    pub on_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

impl CompletionRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
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

    pub fn with_init_state(mut self, s: impl Into<String>) -> Self {
        self.init_state = Some(s.into());
        self
    }

    pub fn with_state_slot(mut self, s: impl Into<String>) -> Self {
        self.state_slot = Some(s.into());
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

/// State-tuning for RNN-based backends (RWKV).
///
/// Feed text through the model (no generation) to prime the recurrent state.
/// Separated from ModelBackend so transformer backends don't need it.
#[async_trait]
pub trait StateTuning: ModelBackend {
    /// Feed text through the model and save the resulting state.
    /// init_state: load from this cache slot (None = blank).
    /// state_slot: save resulting state here (None = don't cache).
    async fn bake(
        &self,
        text: &str,
        init_state: Option<&str>,
        state_slot: Option<&str>,
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

    fn name(&self) -> &str {
        "mock"
    }
}

#[async_trait]
impl StateTuning for MockBackend {
    async fn bake(
        &self,
        _text: &str,
        _init_state: Option<&str>,
        _state_slot: Option<&str>,
    ) -> RoCoResult<String> {
        Err(crate::error::RoCoError::Backend(
            "bake not supported by MockBackend".into(),
        ))
    }
}
