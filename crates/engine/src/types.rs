//! Core types for the inference pipeline.
//!
//! Defines [`CompletionRequest`], [`CompletionResponse`], [`EngineError`],
//! [`TokenUsage`], and [`TokenCounter`].

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Typed string wrapper for validated prompt text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PromptText(pub String);

impl PromptText {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for PromptText {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for PromptText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for PromptText {
    fn from(s: &str) -> Self {
        PromptText(s.to_string())
    }
}

impl From<String> for PromptText {
    fn from(s: String) -> Self {
        PromptText(s)
    }
}

/// Typed string wrapper for session and state cache identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct StateKey(pub String);

impl StateKey {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for StateKey {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for StateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for StateKey {
    fn from(s: &str) -> Self {
        StateKey(s.to_string())
    }
}

impl From<String> for StateKey {
    fn from(s: String) -> Self {
        StateKey(s)
    }
}

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

impl EngineError {
    /// Return a human-readable help string suggesting how to fix this error.
    pub fn help(&self) -> Option<&'static str> {
        match self {
            EngineError::Backend(msg) => {
                if msg.contains("adapter") || msg.contains("context") || msg.contains("GPU") {
                    Some("Try: RWKV_ADAPTER=llvmpipe for CPU fallback, or check Vulkan drivers with `roco gpu-check`")
                } else if msg.contains("model") || msg.contains("load") {
                    Some("Try: Place a .st file in models/ or set $RWKV_MODEL to the model path")
                } else if msg.contains("tokenizer") || msg.contains("vocab") {
                    Some("Try: Set $RWKV_VOCAB to the vocab JSON path, or run from the project root")
                } else if msg.contains("timeout") || msg.contains("hang") {
                    Some("Try: Set RWKV_BACKEND_TIMEOUT for a longer wait, or RWKV_ADAPTER=llvmpipe")
                } else if msg.contains("channel") || msg.contains("shut") {
                    Some("Try: Restart the backend with `roco inferd restart`")
                } else {
                    None
                }
            }
            EngineError::EmptyResponse => {
                Some("Try: Rephrase your prompt or increase max_tokens")
            }
            EngineError::BudgetExceeded { .. } => {
                Some("Try: Increase max_tokens or reduce the prompt length")
            }
            EngineError::TimedOut { .. } => {
                Some("Try: Increase deadline_ms or check GPU load. Set RWKV_DEADLINE_MS=0 for no deadline")
            }
        }
    }
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
///
/// inferd receives raw text only — no System/User/Assistant formatting.
/// All formatting is the caller's responsibility. State is managed
/// explicitly via init_state (load from cache) and state_slot (save to cache).
/// None = start blank / don't cache.
#[derive(Serialize, Deserialize)]
pub struct CompletionRequest {
    pub prompt: String,
    /// Text appended after the prompt so the model sees it as its own
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
    /// Load state from this cache slot before processing. None = start blank.
    pub init_state: Option<String>,
    /// Save resulting state to this cache slot. None = don't cache.
    pub state_slot: Option<String>,
    #[serde(skip)]
    pub on_token: OnToken,
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
    /// Deterministic seed for reproducible sampling.
    /// When `Some(seed)`, the RNG is seeded deterministically so that
    /// the same prompt + seed + temperature produces the same output.
    /// When `None`, uses a truly random seed (current default behaviour).
    #[serde(default)]
    pub seed: Option<u64>,
    /// If true, record per-token sampling metadata into the response.
    /// Enables token-level trace logging for debugging bad generations.
    /// Default: false (no trace, zero memory overhead).
    #[serde(default)]
    pub record_trace: bool,
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
            prompt: self.prompt.clone(),
            prefill: self.prefill.clone(),
            output_schema: self.output_schema.clone(),
            grammar: self.grammar.clone(),
            temperature: self.temperature,
            top_a: self.top_a,
            max_tokens: self.max_tokens,
            estimated_prompt_tokens: self.estimated_prompt_tokens,
            init_state: self.init_state.clone(),
            state_slot: self.state_slot.clone(),
            deadline_ms: self.deadline_ms,
            seed: self.seed,
            record_trace: self.record_trace,
            on_token: None,
            bnf_mask: None,
        }
    }
}

impl std::fmt::Debug for CompletionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompletionRequest")
            .field("prompt", &self.prompt)
            .field("prefill", &self.prefill)
            .field("output_schema", &self.output_schema)
            .field("grammar", &self.grammar)
            .field("temperature", &self.temperature)
            .field("top_a", &self.top_a)
            .field("max_tokens", &self.max_tokens)
            .field("estimated_prompt_tokens", &self.estimated_prompt_tokens)
            .field("init_state", &self.init_state)
            .field("state_slot", &self.state_slot)
            .field("deadline_ms", &self.deadline_ms)
            .field("seed", &self.seed)
            .field("record_trace", &self.record_trace)
            .field("on_token", &self.on_token.as_ref().map(|_| "<callback>"))
            .field("bnf_mask", &self.bnf_mask.as_ref().map(|_| "<BnfMask>"))
            .finish()
    }
}

// Duplicate impl removed - resolved E0119 conflicting implementations of trait `Debug` for type `types::CompletionRequest`

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            prefill: None,
            output_schema: None,
            grammar: None,
            temperature: 0.2,
            top_a: None,
            max_tokens: 512,
            estimated_prompt_tokens: 0,
            init_state: None,
            state_slot: None,
            on_token: None,
            deadline_ms: 0,
            seed: None,
            record_trace: false,
            bnf_mask: None,
        }
    }
}

impl CompletionRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            ..Default::default()
        }
    }

    /// Create a builder for convenient construction with validation.
    pub fn builder() -> CompletionRequestBuilder {
        CompletionRequestBuilder::default()
    }
}

/// Builder pattern for [`CompletionRequest`].
///
/// Encapsulates common presets and applies env-var overrides automatically.
#[derive(Default)]
pub struct CompletionRequestBuilder {
    prompt: Option<String>,
    prefill: Option<String>,
    output_schema: Option<String>,
    grammar: Option<String>,
    temperature: Option<f32>,
    top_a: Option<f32>,
    max_tokens: Option<usize>,
    init_state: Option<String>,
    state_slot: Option<String>,
    deadline_ms: Option<u64>,
    seed: Option<u64>,
    /// NOTE: OnToken is already Option<Box<dyn Fn...>>, so we store it directly.
    on_token: OnToken,
    bnf_mask: Option<Box<dyn BnfMask>>,
    record_trace: Option<bool>,
}

impl std::fmt::Debug for CompletionRequestBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompletionRequestBuilder")
            .field("prompt", &self.prompt)
            .field("prefill", &self.prefill)
            .field("output_schema", &self.output_schema)
            .field("grammar", &self.grammar)
            .field("temperature", &self.temperature)
            .field("top_a", &self.top_a)
            .field("max_tokens", &self.max_tokens)
            .field("init_state", &self.init_state)
            .field("state_slot", &self.state_slot)
            .field("deadline_ms", &self.deadline_ms)
            .field("seed", &self.seed)
            .field("record_trace", &self.record_trace)
            .field("on_token", &self.on_token.as_ref().map(|_| "<callback>"))
            .field("bnf_mask", &self.bnf_mask.as_ref().map(|_| "<BnfMask>"))
            .finish()
    }
}

impl Clone for CompletionRequestBuilder {
    fn clone(&self) -> Self {
        Self {
            prompt: self.prompt.clone(),
            prefill: self.prefill.clone(),
            output_schema: self.output_schema.clone(),
            grammar: self.grammar.clone(),
            temperature: self.temperature,
            top_a: self.top_a,
            max_tokens: self.max_tokens,
            init_state: self.init_state.clone(),
            state_slot: self.state_slot.clone(),
            deadline_ms: self.deadline_ms,
            seed: self.seed,
            record_trace: self.record_trace,
            on_token: None,
            bnf_mask: None,
        }
    }
}

impl CompletionRequestBuilder {
    /// Set the user prompt.
    pub fn prompt(mut self, p: impl Into<String>) -> Self {
        self.prompt = Some(p.into());
        self
    }

    /// Set prefill text injected after the prompt.
    pub fn prefill(mut self, p: impl Into<String>) -> Self {
        self.prefill = Some(p.into());
        self
    }

    /// Set output schema for structured generation.
    pub fn output_schema(mut self, s: impl Into<String>) -> Self {
        self.output_schema = Some(s.into());
        self
    }

    /// Set grammar for constrained decoding.
    pub fn grammar(mut self, g: impl Into<String>) -> Self {
        self.grammar = Some(g.into());
        self
    }

    /// Set grammar for constrained decoding from an optional string.
    pub fn grammar_opt(mut self, g: Option<String>) -> Self {
        self.grammar = g;
        self
    }

    /// Set sampling temperature (default: 0.2).
    pub fn temperature(mut self, t: f32) -> Self {
        self.temperature = Some(t);
        self
    }

    /// Set top_a sampling parameter.
    pub fn top_a(mut self, a: f32) -> Self {
        self.top_a = Some(a);
        self
    }

    /// Set maximum tokens to generate.
    pub fn max_tokens(mut self, n: usize) -> Self {
        self.max_tokens = Some(n);
        self
    }

    /// Load state from this cache slot before processing. None = start blank.
    pub fn init_state(mut self, s: impl Into<String>) -> Self {
        self.init_state = Some(s.into());
        self
    }

    /// Save resulting state to this cache slot. None = don't cache.
    pub fn state_slot(mut self, s: impl Into<String>) -> Self {
        self.state_slot = Some(s.into());
        self
    }

    /// Set wall-clock deadline in milliseconds (0 = no deadline).
    pub fn deadline_ms(mut self, ms: u64) -> Self {
        self.deadline_ms = Some(ms);
        self
    }

    /// Set deterministic seed for reproducible sampling.
    pub fn seed(mut self, s: u64) -> Self {
        self.seed = Some(s);
        self
    }

    /// Set a token callback for streaming.
    pub fn on_token(mut self, cb: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_token = Some(Box::new(cb));
        self
    }

    /// Set a token callback option for streaming.
    pub fn on_token_opt(mut self, cb: OnToken) -> Self {
        self.on_token = cb;
        self
    }

    /// Set a BNF grammar mask.
    pub fn bnf_mask(mut self, mask: Box<dyn BnfMask>) -> Self {
        self.bnf_mask = Some(mask);
        self
    }

    /// Enable per-token trace recording for debugging.
    pub fn record_trace(mut self, enabled: bool) -> Self {
        self.record_trace = Some(enabled);
        self
    }

    /// Apply a grammar-constrained preset.
    pub fn grammar_preset(mut self, gbnf: impl Into<String>) -> Self {
        self.grammar = Some(gbnf.into());
        self.temperature.get_or_insert(0.2);
        self.max_tokens.get_or_insert(512);
        self
    }

    /// Apply env-var overrides for determinism environment variables.
    fn apply_env_overrides(mut self) -> Self {
        // RWKV_DETERMINISTIC_SEED overrides if set and no explicit seed given
        if self.seed.is_none() {
            if let Ok(s) = std::env::var("RWKV_DETERMINISTIC_SEED") {
                if let Ok(seed) = s.parse::<u64>() {
                    self.seed = Some(seed);
                }
            }
        }
        // RWKV_TEMPERATURE overrides if set and no explicit temperature given
        if self.temperature.is_none() {
            if let Ok(s) = std::env::var("RWKV_TEMPERATURE") {
                if let Ok(t) = s.parse::<f32>() {
                    self.temperature = Some(t);
                }
            }
        }
        self
    }

    /// Build the final [`CompletionRequest`].
    ///
    /// Panics if `prompt` is not set. Call `prompt(...)` before `build()`.
    pub fn build(self) -> CompletionRequest {
        let b = self.apply_env_overrides();
        CompletionRequest {
            prompt: b
                .prompt
                .expect("CompletionRequestBuilder: prompt is required"),
            prefill: b.prefill,
            output_schema: b.output_schema,
            grammar: b.grammar,
            temperature: b.temperature.unwrap_or(0.2),
            top_a: b.top_a,
            max_tokens: b.max_tokens.unwrap_or(512),
            estimated_prompt_tokens: 0,
            init_state: b.init_state,
            state_slot: b.state_slot,
            deadline_ms: b.deadline_ms.unwrap_or(0),
            seed: b.seed,
            record_trace: b.record_trace.unwrap_or(false),
            on_token: b.on_token,
            bnf_mask: b.bnf_mask,
        }
    }
}

/// Per-token sampling metadata for trace logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTrace {
    pub token_id: u32,
    pub token_str: String,
    pub probability: f32,
    pub temperature: f32,
    pub top_p_cut: f32,
    pub grammar_masked: bool,
    pub selected_by_grammar: bool,
}

/// A completion produced by a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    pub usage: TokenUsage,
    pub parsed: Option<serde_json::Value>,
    /// Per-token trace metadata. Only populated when the corresponding
    /// `CompletionRequest::record_trace` is true.
    #[serde(default)]
    pub trace: Vec<TokenTrace>,
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
        let req = CompletionRequest::new("user message");
        assert_eq!(req.prompt, "user message");
        assert_eq!(req.temperature, 0.2);
        assert_eq!(req.max_tokens, 512);
    }

    #[test]
    fn completion_request_deserialize_minimal() {
        let json = r#"{"prompt": "hello", "temperature": 0.5, "max_tokens": 10}"#;
        let req: CompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "hello");
        assert_eq!(req.temperature, 0.5);
        assert_eq!(req.max_tokens, 10);
    }

    // ── Builder tests ───────────────────────────────────────────────────

    #[test]
    fn builder_requires_prompt() {
        let req = CompletionRequest::builder().prompt("hello").build();
        assert_eq!(req.prompt, "hello");
        assert_eq!(req.temperature, 0.2); // default
        assert!(req.seed.is_none());
    }

    #[test]
    fn builder_sets_all_fields() {
        let req = CompletionRequest::builder()
            .prompt("Tell me a story")
            .temperature(0.8)
            .max_tokens(2048)
            .seed(42)
            .top_a(0.5)
            .init_state("story-writer")
            .state_slot("story-writer")
            .deadline_ms(30000)
            .grammar("story")
            .output_schema("json")
            .prefill("Once upon a time")
            .build();

        assert_eq!(req.prompt, "Tell me a story");
        assert!((req.temperature - 0.8).abs() < 1e-6);
        assert_eq!(req.max_tokens, 2048);
        assert_eq!(req.seed, Some(42));
        assert!((req.top_a.unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(req.init_state.as_deref(), Some("story-writer"));
        assert_eq!(req.state_slot.as_deref(), Some("story-writer"));
        assert_eq!(req.deadline_ms, 30000);
        assert_eq!(req.grammar.as_deref(), Some("story"));
        assert_eq!(req.output_schema.as_deref(), Some("json"));
        assert_eq!(req.prefill.as_deref(), Some("Once upon a time"));
    }

    #[test]
    fn builder_grammar_preset_applies_defaults() {
        let req = CompletionRequest::builder()
            .prompt("output json")
            .grammar_preset("root ::= \"hello\"")
            .build();
        assert_eq!(req.temperature, 0.2);
        assert_eq!(req.max_tokens, 512);
        assert_eq!(req.grammar.as_deref(), Some("root ::= \"hello\""));
    }

    #[test]
    #[should_panic(expected = "prompt is required")]
    fn builder_panics_without_prompt() {
        let _req = CompletionRequest::builder().build();
    }

    // ── Pending tests for features not yet implemented ──────────────────

    /// TokenTrace serialization round-trip (pending trace feature).
    #[test]
    #[ignore = "pending TokenTrace serialization feature"]
    fn trace_serialization_roundtrip() {
        let trace = TokenTrace {
            token_id: 42,
            token_str: "hello".to_string(),
            probability: 0.95,
            temperature: 0.7,
            top_p_cut: 0.9,
            grammar_masked: true,
            selected_by_grammar: true,
        };
        let json = serde_json::to_string(&trace).unwrap();
        let roundtrip: TokenTrace = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.token_id, 42);
        assert!((roundtrip.probability - 0.95).abs() < 1e-6);
    }

    /// CompletionResponse with trace field serializes correctly.
    #[test]
    #[ignore = "pending trace serialization feature"]
    fn response_with_trace_serializes() {
        let resp = CompletionResponse {
            text: "hello world".to_string(),
            usage: TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 3,
            },
            parsed: None,
            trace: vec![TokenTrace {
                token_id: 1,
                token_str: "hello".to_string(),
                probability: 0.9,
                temperature: 0.5,
                top_p_cut: 0.95,
                grammar_masked: false,
                selected_by_grammar: true,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("hello world"));
        assert!(json.contains("trace"));
    }

    /// EngineError help text for all variants.
    #[test]
    #[ignore = "pending comprehensive error help text"]
    fn engine_error_help_text_coverage() {
        let backend_err = EngineError::Backend("adapter".to_string());
        assert!(backend_err.help().is_some());

        let empty = EngineError::EmptyResponse;
        assert!(empty.help().is_some());

        let budget = EngineError::BudgetExceeded {
            used: 100,
            max: 200,
        };
        assert!(budget.help().is_some());

        let timeout = EngineError::TimedOut { ms: 5000 };
        assert!(timeout.help().is_some());
    }

    /// Session blending with three or more states.
    #[test]
    #[ignore = "pending multi-state blend feature"]
    fn blend_three_states() {
        // Placeholder: requires blend_states to support 3+ states
        // Currently only 2-state blend is implemented
    }

    /// Grammar-constrained response with invalid token rejection.
    #[test]
    #[ignore = "pending grammar mask testing"]
    fn grammar_mask_rejects_invalid_tokens() {
        // Placeholder: requires a concrete grammar and token validation
    }
}
