//! [`ModelBackend`] trait — the inference seam that every backend implements.
//!
//! [`MockBackend`] is provided for testing without a real model.

use futures::future::BoxFuture;

use crate::types::*;

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

    /// Serialize the current model state (recurrent hidden state) to bytes.
    /// Returns `Err(EngineError::Backend("state not supported"))` by default.
    fn save_state(&self) -> BoxFuture<'_, Result<Vec<u8>, EngineError>> {
        Box::pin(async move { Err(EngineError::Backend("state not supported".into())) })
    }

    /// Restore model state from previously saved bytes.
    fn load_state(&self, _state: Vec<u8>) -> BoxFuture<'_, Result<(), EngineError>> {
        Box::pin(async move { Err(EngineError::Backend("state not supported".into())) })
    }

    /// Blend two saved states with a linear ratio.
    fn mix_states(
        &self,
        _state_a: Vec<u8>,
        _state_b: Vec<u8>,
        _ratio: f32,
    ) -> BoxFuture<'_, Result<Vec<u8>, EngineError>> {
        Box::pin(async move { Err(EngineError::Backend("state mixing not supported".into())) })
    }

    /// Request cancellation of the current in-flight generation.
    fn interrupt(&self) -> BoxFuture<'_, Result<(), EngineError>> {
        Box::pin(async move { Err(EngineError::Backend("interrupt not supported".into())) })
    }

    /// Return the model's vocabulary as per-token byte sequences, used to
    /// build BNF grammar masks. Returns `None` by default for backends that
    /// don't expose their vocab (e.g. `MockBackend`).
    fn vocab_bytes(&self) -> Option<Vec<Vec<u8>>> {
        None
    }
}

/// Deterministic backend for tests / pre-model development.
///
/// Supports simulated failures via `fail_count` — the first N calls will
/// return `EngineError::Backend(...)`, then subsequent calls succeed.
#[derive(Debug)]
pub struct MockBackend {
    pub name: String,
    pub latency_ms: u64,
    /// Number of times `complete()` will fail before succeeding.
    pub fail_count: u32,
    fail_count_remaining: std::sync::atomic::AtomicU32,
}

impl Clone for MockBackend {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            latency_ms: self.latency_ms,
            fail_count: self.fail_count,
            fail_count_remaining: std::sync::atomic::AtomicU32::new(
                self.fail_count_remaining
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
        }
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self {
            name: "mock-3b".into(),
            latency_ms: 0,
            fail_count: 0,
            fail_count_remaining: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

impl MockBackend {
    /// Create a new MockBackend with the given name and fail count.
    /// Create a new MockBackend with the given name and fail count.
    /// The first `fail_count` calls to `complete()` will fail.
    pub fn new(name: &str, fail_count: u32) -> Self {
        Self {
            name: name.into(),
            latency_ms: 0,
            fail_count,
            fail_count_remaining: std::sync::atomic::AtomicU32::new(fail_count),
        }
    }

    /// Reset the fail counter so the next N calls will fail.
    pub fn set_fail_count(&mut self, count: u32) {
        self.fail_count = count;
        self.fail_count_remaining
            .store(count, std::sync::atomic::Ordering::Relaxed);
    }
}

impl ModelBackend for MockBackend {
    fn name(&self) -> &str {
        &self.name
    }
    fn complete(
        &self,
        req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            if self.latency_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.latency_ms)).await;
            }
            // Simulate failures — check counter before decrementing to avoid wraparound.
            if self
                .fail_count_remaining
                .load(std::sync::atomic::Ordering::Relaxed)
                > 0
            {
                self.fail_count_remaining
                    .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                return Err(EngineError::Backend("simulated failure".into()));
            }
            let snippet: String = req.prompt.chars().take(48).collect();
            let text =
                serde_json::json!({ "result": format!("[{}] {}", self.name, snippet) }).to_string();
            let parsed = serde_json::from_str(&text).ok();

            let (text, think_trace) = if req.thinking {
                let trace = format!("thinking about '{}'...", snippet);
                (format!("<think>{}</think>\n{}", trace, text), Some(trace))
            } else {
                (text, None)
            };

            Ok(CompletionResponse {
                text,
                usage: TokenUsage {
                    prompt_tokens: req.estimated_prompt_tokens,
                    completion_tokens: 16,
                },
                parsed,
                think_trace,
            })
        })
    }

    fn save_state(&self) -> BoxFuture<'_, Result<Vec<u8>, EngineError>> {
        let name = self.name.clone();
        Box::pin(async move {
            let state = serde_json::json!({
                "backend": name,
                "mock_state": true,
            });
            Ok(serde_json::to_vec(&state).unwrap())
        })
    }

    fn load_state(&self, state: Vec<u8>) -> BoxFuture<'_, Result<(), EngineError>> {
        Box::pin(async move {
            let _state: serde_json::Value = serde_json::from_slice(&state)
                .map_err(|e| EngineError::Backend(format!("invalid mock state: {e}")))?;
            Ok(())
        })
    }

    fn mix_states(
        &self,
        state_a: Vec<u8>,
        state_b: Vec<u8>,
        ratio: f32,
    ) -> BoxFuture<'_, Result<Vec<u8>, EngineError>> {
        Box::pin(async move {
            let a: serde_json::Value = serde_json::from_slice(&state_a)
                .map_err(|e| EngineError::Backend(format!("invalid state_a: {e}")))?;
            let b: serde_json::Value = serde_json::from_slice(&state_b)
                .map_err(|e| EngineError::Backend(format!("invalid state_b: {e}")))?;
            let merged = serde_json::json!({
                "backend": a.get("backend").or_else(|| b.get("backend")),
                "mock_state": true,
                "mixed_ratio": ratio,
                "source_a": a,
                "source_b": b,
            });
            Ok(serde_json::to_vec(&merged).unwrap())
        })
    }

    fn interrupt(&self) -> BoxFuture<'_, Result<(), EngineError>> {
        Box::pin(async move { Ok(()) })
    }
}

/// Run example turns through a backend with `preserve_state` and return
/// the final hidden state.
pub async fn bake_persona(
    backend: &dyn ModelBackend,
    system: &str,
    examples: &[(&str, &str)],
) -> Result<Vec<u8>, EngineError> {
    for (i, (user_msg, assistant_msg)) in examples.iter().enumerate() {
        let req = CompletionRequest {
            system: if i == 0 {
                system.to_string()
            } else {
                String::new()
            },
            prompt: user_msg.to_string(),
            temperature: 0.0,
            max_tokens: 1024,
            preserve_state: i > 0,
            ..Default::default()
        };
        backend.complete(req).await?;
        let req_assistant = CompletionRequest {
            prefill: Some(assistant_msg.to_string()),
            temperature: 0.0,
            max_tokens: 1024,
            preserve_state: true,
            ..Default::default()
        };
        backend.complete(req_assistant).await?;
    }
    backend.save_state().await
}

/// Bake a few-shot persona into a *named session* by replaying example turns
/// through the backend with `preserve_state` enabled.
///
/// Unlike [`bake_persona`], which returns raw state bytes (only meaningful for
/// backends that implement `save_state`/`load_state`), this uses the session
/// mechanism (`CompletionRequest::session` + `preserve_state`) so the
/// recurrent state of that session — not a rebuilt prompt — carries the
/// persona. This is what the chat CLI uses, because `RwkvBackend` manages
/// state through its session pool rather than byte snapshots.
///
/// The first example's user turn folds in `system`; every subsequent turn
/// relies on the accumulated state. After baking, mark the session as `baked`
/// so later user turns don't re-send the system prompt or the examples.
pub async fn bake_into_session(
    backend: &dyn ModelBackend,
    session: &str,
    system: &str,
    examples: &[(&str, &str)],
) -> Result<(), EngineError> {
    for (i, (user_msg, assistant_msg)) in examples.iter().enumerate() {
        let user_req = CompletionRequest {
            system: if i == 0 {
                system.to_string()
            } else {
                String::new()
            },
            prompt: user_msg.to_string(),
            temperature: 0.0,
            max_tokens: 1, // State-tuning: only need prompt processing, not generation
            preserve_state: true,
            session: Some(session.to_string()),
            ..Default::default()
        };
        backend.complete(user_req).await?;
        let asst_req = CompletionRequest {
            system: String::new(),
            prompt: assistant_msg.to_string(),
            temperature: 0.0,
            max_tokens: 1, // State-tuning: only need prompt processing, not generation
            preserve_state: true,
            session: Some(session.to_string()),
            ..Default::default()
        };
        backend.complete(asst_req).await?;
    }
    Ok(())
}

/// Prefill that closes the think channel immediately, so generation starts in
/// *content* mode rather than planning mode.
///
/// Derived from `prompt_probe_eval`: after `Assistant: <think></think>` the
/// model emits content and does **not** re-open `<think>`. Without any prefill
/// a bare `Assistant:` start defaults to an open `<think>` block (the source
/// of think-tag contamination in the story pipeline). System-prompt
/// instructions like "never use think tags" backfire — they merely prime the
/// model to emit `<think>`, so they must not be used.
///
/// NOTE: this prefill contains `<`/`>` and therefore cannot be combined with a
/// grammar that forbids those characters (e.g. JSON-envelope grammars). For
/// grammar-constrained generation, use [`bake_no_think_session`] instead and
/// rely on the baked recurrent state to bias the opening token toward `{`.
pub const NO_THINK_PREFILL: &str = "<think></think>";

/// Bake a *no-think* session by replaying (user, assistant) turns where the
/// assistant turn is injected as a **prefill** (the correct assistant role),
/// so the recurrent state learns that assistant responses begin with content,
/// never `<think>`.
///
/// This is the correctly-roled counterpart of [`bake_into_session`], which
/// feeds the assistant text through `prompt` (the user role) and therefore
/// leaves the baked state expecting another *user* turn — probe experiments
/// showed that mistake makes the model emit spurious `User:` turns.
pub async fn bake_no_think_session(
    backend: &dyn ModelBackend,
    session: &str,
    system: &str,
    examples: &[(&str, &str)],
) -> Result<(), EngineError> {
    for (i, (user_msg, assistant_msg)) in examples.iter().enumerate() {
        let user_req = CompletionRequest {
            system: if i == 0 {
                system.to_string()
            } else {
                String::new()
            },
            prompt: user_msg.to_string(),
            temperature: 0.0,
            max_tokens: 1, // State-tuning: only need prompt processing, not generation
            preserve_state: true,
            session: Some(session.to_string()),
            ..Default::default()
        };
        backend.complete(user_req).await?;
        let asst_req = CompletionRequest {
            system: String::new(),
            prefill: Some(assistant_msg.to_string()),
            temperature: 0.0,
            max_tokens: 1, // State-tuning: only need prompt processing, not generation
            preserve_state: true,
            session: Some(session.to_string()),
            ..Default::default()
        };
        backend.complete(asst_req).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_backend_returns_parseable_json() {
        let b = MockBackend::default();
        let resp = b
            .complete(CompletionRequest::new("sys", "do the thing"))
            .await
            .unwrap();
        assert!(resp.parsed.is_some());
        assert!(resp.text.contains("mock") || resp.text.contains("result"));
    }

    #[tokio::test]
    async fn mock_backend_thinking_extracts_trace() {
        let b = MockBackend::default();
        let resp = b
            .complete(CompletionRequest::new("sys", "hello"))
            .await
            .unwrap();
        assert!(resp.think_trace.is_none(), "no trace when thinking=false");

        let mut req = CompletionRequest::new("sys", "do the thing");
        req.thinking = true;
        let resp = b.complete(req).await.unwrap();
        let trace = resp
            .think_trace
            .expect("think_trace should be Some when thinking=true");
        assert!(!trace.is_empty());
        assert!(resp.text.starts_with("<think>"));
        assert!(resp.text.contains("</think>"));
        assert!(resp.text.contains(&trace));
    }

    #[tokio::test]
    async fn mock_backend_save_load_state() {
        let b = MockBackend::default();
        let state = b.save_state().await.unwrap();
        assert!(!state.is_empty());
        b.load_state(state).await.unwrap();
        let err = b.load_state(b"trash".to_vec()).await.unwrap_err();
        assert!(format!("{err:?}").contains("invalid mock state"));
    }

    #[tokio::test]
    async fn default_backend_rejects_state() {
        struct NoStateBackend;
        impl ModelBackend for NoStateBackend {
            fn name(&self) -> &str {
                "no-state"
            }
            fn complete(
                &self,
                _req: CompletionRequest,
            ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
                Box::pin(async move { Err(EngineError::Backend("unimplemented".into())) })
            }
        }
        let b = NoStateBackend;
        assert!(format!("{:?}", b.save_state().await.unwrap_err()).contains("state not supported"));
        assert!(format!("{:?}", b.load_state(Vec::new()).await.unwrap_err())
            .contains("state not supported"));
        assert!(format!(
            "{:?}",
            b.mix_states(Vec::new(), Vec::new(), 0.5).await.unwrap_err()
        )
        .contains("state mixing not supported"));
        assert!(
            format!("{:?}", b.interrupt().await.unwrap_err()).contains("interrupt not supported")
        );
    }

    #[tokio::test]
    async fn mock_backend_mix_states() {
        let b = MockBackend::default();
        let a = b.save_state().await.unwrap();
        let mut req_b = CompletionRequest::new("sys", "hello");
        req_b.thinking = true;
        let _ = b.complete(req_b).await.unwrap();
        let b_state = b.save_state().await.unwrap();
        let mixed = b.mix_states(a, b_state, 0.3).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&mixed).unwrap();
        assert!((v["mixed_ratio"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        assert!(v.get("source_a").is_some());
        assert!(v.get("source_b").is_some());
    }

    #[tokio::test]
    async fn mock_backend_interrupt() {
        let b = MockBackend::default();
        b.interrupt().await.unwrap();
        let resp = b
            .complete(CompletionRequest::new("sys", "hello"))
            .await
            .unwrap();
        assert!(resp.text.contains("result"));
    }

    #[tokio::test]
    async fn bake_persona_produces_usable_state() {
        let b = MockBackend::default();
        let examples = [
            ("What is your name?", "My name is Mock."),
            ("What can you do?", "I can help with many things."),
        ];
        let state = bake_persona(&b, "You are a helpful assistant.", &examples)
            .await
            .unwrap();
        assert!(!state.is_empty());
        b.load_state(state).await.unwrap();
    }

    #[tokio::test]
    async fn bake_into_session_replays_examples_on_named_session() {
        let b = MockBackend::default();
        let session = "persona-session";
        let examples = [
            ("Hi there.", "Hello! How can I help?"),
            ("Who are you?", "I am a polite assistant."),
        ];
        // Baking replays each example as two turns on the named session.
        bake_into_session(&b, session, "You are a polite assistant.", &examples)
            .await
            .unwrap();
        // The session state is retrievable and a follow-up turn completes.
        let state = b.save_state().await.unwrap();
        assert!(!state.is_empty());
        let resp = b
            .complete(CompletionRequest {
                prompt: "Thanks!".into(),
                preserve_state: true,
                session: Some(session.into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(resp.text.contains("result"));
    }
}
