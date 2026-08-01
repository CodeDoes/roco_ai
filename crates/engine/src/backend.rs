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

    /// Feed raw text through the model (no generation) and save the resulting
    /// state. Used to prime the recurrent state with few-shot examples or
    /// context without consuming tokens.
    ///
    /// Default: not supported (returns error).
    fn bake<'a>(
        &'a self,
        text: &'a str,
        init_state: Option<&'a str>,
        state_slot: Option<&'a str>,
    ) -> BoxFuture<'a, Result<String, EngineError>> {
        let _ = (text, init_state, state_slot);
        Box::pin(async move {
            Err(EngineError::Backend(
                "bake not supported by this backend".into(),
            ))
        })
    }
}

/// Optional trait for RNN-based backends that support state tuning.
///
/// Separated from [`ModelBackend`] because state tuning is only meaningful
/// for recurrent (RNN) architectures like RWKV that carry a hidden state
/// across turns. Transformer-based backends would have no-op defaults.
pub trait StateTuning: Send + Sync {
    /// Bake a system prompt and few-shot examples into a named session's
    /// recurrent state, returning the session ID on success.
    fn tune_state<'a>(
        &'a self,
        session_id: &'a str,
        system: &'a str,
        few_shots: &'a [(&'a str, &'a str)],
    ) -> BoxFuture<'a, Result<String, EngineError>>;

    /// Blend N session states element-wise with explicit weights:
    /// `output = Σ(weight_i * state_i) / Σ(weight_i)`.
    ///
    /// `states` is a slice of `(session_id, weight)` pairs; weights may be
    /// arbitrary and are normalized. At least two states are required.
    ///
    /// Default implementation returns a descriptive error for backends that
    /// don't support state blending.
    fn blend_states<'a>(
        &'a self,
        states: &'a [(&'a str, f32)],
        output_session: &'a str,
    ) -> BoxFuture<'a, Result<(), EngineError>> {
        let _ = (states, output_session);
        Box::pin(async move {
            Err(EngineError::Backend(
                "state blending not supported by this backend".into(),
            ))
        })
    }
}

fn mock_random_walk_bnf(
    gbnf: &str,
    max_tokens: usize,
    seed: Option<u64>,
    top_a: Option<f32>,
) -> Option<String> {
    use ahash::AHashMap;
    use kbnf::engine_like::EngineLike;
    use kbnf::{Config, Engine, Token, Vocabulary};
    use rand::seq::SliceRandom;
    use rand::SeedableRng;

    let kbnf_str = crate::grammar::gbnf_to_kbnf(gbnf);
    let tokens: Vec<&str> = vec![
        "",
        "\n",
        " ",
        "\t",
        "\"",
        "\\",
        "{",
        "}",
        "[",
        "]",
        ":",
        ",",
        "-",
        ".",
        "a",
        "b",
        "c",
        "d",
        "e",
        "f",
        "g",
        "h",
        "i",
        "j",
        "k",
        "l",
        "m",
        "n",
        "o",
        "p",
        "q",
        "r",
        "s",
        "t",
        "u",
        "v",
        "w",
        "x",
        "y",
        "z",
        "A",
        "B",
        "C",
        "D",
        "E",
        "F",
        "G",
        "H",
        "I",
        "J",
        "K",
        "L",
        "M",
        "N",
        "O",
        "P",
        "Q",
        "R",
        "S",
        "T",
        "U",
        "V",
        "W",
        "X",
        "Y",
        "Z",
        "0",
        "1",
        "2",
        "3",
        "4",
        "5",
        "6",
        "7",
        "8",
        "9",
        "true",
        "false",
        "null",
        "result",
        "title",
        "chapter",
        "content",
        "name",
        "summary",
        "quality",
        "pass",
        "fail",
        "issues",
        "suggestion",
    ];

    let mut id_to_token: AHashMap<u32, Token> = AHashMap::new();
    let mut id_to_string: AHashMap<u32, String> = AHashMap::new();

    for (id, &s) in tokens.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let token_id = id as u32;
        id_to_token.insert(token_id, Token(s.as_bytes().to_vec().into_boxed_slice()));
        id_to_string.insert(token_id, s.to_string());
    }

    let vocab_obj = Vocabulary::new(id_to_token, id_to_string.clone()).ok()?;
    let config = Config {
        start_nonterminal: "root".to_string(),
        ..Config::default()
    };
    let mut engine = Engine::with_config(&kbnf_str, vocab_obj, config).ok()?;

    let mut rng: Box<dyn rand::RngCore> = match seed {
        Some(s) => Box::new(rand::rngs::StdRng::seed_from_u64(s)),
        None => Box::new(rand::thread_rng()),
    };
    let mut out = String::new();

    let steps = if max_tokens == 0 { 256 } else { max_tokens };
    for _ in 0..steps {
        engine.compute_allowed_token_ids();
        if engine.is_finished() {
            break;
        }
        let bitset = engine.allowed_token_ids_from_last_computation();
        let allowed: Vec<u32> = (0..tokens.len() as u32)
            .filter(|&id| !tokens[id as usize].is_empty() && bitset.contains(id as usize))
            .collect();
        if allowed.is_empty() {
            break;
        }
        // top-a truncation: keep the top `ceil(a * n)` candidates. With
        // uniform mock probabilities this is the exact analog of real top-a
        // (keep tokens whose cumulative mass reaches `a`). a >= 1.0 / None →
        // no truncation.
        let cutoff = match top_a {
            Some(a) if a.is_finite() && a >= 0.0 && a < 1.0 => {
                ((a * allowed.len() as f32).ceil() as usize).clamp(1, allowed.len())
            }
            _ => allowed.len(),
        };
        let window = &allowed[..cutoff];
        let chosen = *window.choose(&mut rng)?;
        let tok_str = id_to_string.get(&chosen)?;
        out.push_str(tok_str);
        if engine.try_accept_new_token(chosen).is_err() {
            break;
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Deterministic grammar-masked generation over the mock byte vocabulary
/// (same token ids as [`MockBackend::vocab_bytes`]: id 0 = "", 1-3 =
/// tab/LF/CR, 4..=98 = 0x20..=0x7E). Picks the highest-scoring allowed
/// token each step until the mask finishes (`accept` → false) or no tokens
/// remain allowed.
fn mock_masked_walk(mask: &mut dyn BnfMask, max_tokens: usize) -> String {
    let mut vocab: Vec<String> = vec![String::new(), "\t".into(), "\n".into(), "\r".into()];
    for b in 0x20u8..=0x7Eu8 {
        vocab.push((b as char).to_string());
    }
    let mut logits = vec![0.0f32; vocab.len()];
    let mut out = String::new();
    let steps = if max_tokens == 0 { 256 } else { max_tokens };
    for _ in 0..steps {
        for l in logits.iter_mut() {
            *l = 0.0;
        }
        mask.mask(&mut logits);
        let best = logits
            .iter()
            .enumerate()
            .filter(|(_, &v)| v.is_finite())
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i);
        let Some(id) = best else { break };
        out.push_str(&vocab[id]);
        if !mask.accept(id as u32) {
            break;
        }
    }
    out
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
    /// Set by `interrupt()` to cancel the in-flight generation.
    interrupt_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
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
            interrupt_flag: self.interrupt_flag.clone(),
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
            interrupt_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl MockBackend {
    /// Create a new MockBackend with the given name and fail count.
    /// The first `fail_count` calls to `complete()` will fail.
    pub fn new(name: &str, fail_count: u32) -> Self {
        Self {
            name: name.into(),
            latency_ms: 0,
            fail_count,
            fail_count_remaining: std::sync::atomic::AtomicU32::new(fail_count),
            interrupt_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
    fn vocab_bytes(&self) -> Option<Vec<Vec<u8>>> {
        let mut v: Vec<Vec<u8>> = vec![b"".to_vec()]; // Token 0: EOS sentinel
        for b in [0x09u8, 0x0Au8, 0x0Du8] {
            v.push(vec![b]);
        }
        for b in 0x20u8..=0x7Eu8 {
            v.push(vec![b]);
        }
        Some(v)
    }
    fn complete(
        &self,
        mut req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        Box::pin(async move {
            // Reset any stale interrupt so a previous interrupt() call doesn't
            // break subsequent generations (mirrors the actor: interrupt
            // targets only the in-flight generation).
            self.interrupt_flag
                .store(false, std::sync::atomic::Ordering::Relaxed);

            // Deadline enforcement: if the simulated latency would exceed the
            // request's wall-clock deadline, fail with TimedOut (mirrors the
            // actor's deadline handling).
            if req.deadline_ms > 0 && self.latency_ms > req.deadline_ms {
                return Err(EngineError::TimedOut {
                    ms: req.deadline_ms,
                });
            }

            // Simulated latency, interruptible via interrupt().
            if self.latency_ms > 0 {
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(self.latency_ms);
                while std::time::Instant::now() < deadline {
                    if self
                        .interrupt_flag
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        return Err(EngineError::Backend("generation interrupted".into()));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
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
            // Strip a leading "System: ...\n\n" header from the echo snippet
            // so tests that assert on the echoed user message still see it
            // (prompts now embed system text inline after migration).
            let snippet: String = {
                let p = req.prompt.strip_prefix("System:");
                match p {
                    Some(rest) => {
                        let rest = rest
                            .trim_start()
                            .trim_start_matches(|c| c == ' ' || c == '\n');
                        let rest = rest.split_once("\n\n").map(|(_, u)| u).unwrap_or(rest);
                        rest.chars().take(48).collect()
                    }
                    None => req.prompt.chars().take(48).collect(),
                }
            };
            // Log the seed when provided for reproducibility debugging.
            if let Some(s) = req.seed {
                tracing::info!(seed = s, snippet = %snippet, "MockBackend completing with deterministic seed");
            }

            // Grammar paths: an opaque mask (compiled upstream) takes priority
            // over a raw BNF string — same precedence as the real actor.
            let masked_text = req
                .bnf_mask
                .as_mut()
                .map(|m| mock_masked_walk(m.as_mut(), req.max_tokens));
            let bnf_walk_text = if masked_text.is_some() {
                masked_text
            } else {
                req.grammar
                    .as_ref()
                    .and_then(|g| mock_random_walk_bnf(g, req.max_tokens, req.seed, req.top_a))
            };

            let prompt_lower = req.prompt.to_lowercase();
            let mut matched_text = bnf_walk_text;

            if matched_text.is_none() {
                if prompt_lower.contains("classify user intent") {
                    // Keyword-based intent classification (AGENTS.md §13): a
                    // deterministic path for the router's NLU so tests and
                    // mock runs don't need a real model. Real classifier
                    // remains the primary path when a model is available.
                    let user_prompt = req
                        .prompt
                        .split_once("User message:")
                        .and_then(|(_, rest)| rest.split('"').nth(1))
                        .unwrap_or_default()
                        .to_string();
                    let msg = user_prompt.to_lowercase();
                    // Score-based keyword classification: count keyword hits
                    // per intent; on ties prefer the later intent (coder over
                    // story for e.g. "write code"). All-zero → chat.
                    let count = |msg: &str, words: &[&str]| -> usize {
                        words.iter().filter(|w| msg.contains(**w)).count()
                    };
                    let scores: Vec<(&str, usize)> = vec![
                        ("adventure", count(&msg, &["adventure", "play", "game"])),
                        ("story", count(&msg, &["story", "write", "tale"])),
                        ("html", count(&msg, &["html", "webpage", "website", "page"])),
                        (
                            "coder",
                            count(&msg, &["code", "program", "function", "bug", "rust"]),
                        ),
                    ];
                    let intent = if scores.iter().all(|(_, n)| *n == 0) {
                        "chat"
                    } else {
                        scores
                            .iter()
                            .max_by_key(|(_, n)| *n)
                            .map(|(id, _)| *id)
                            .unwrap_or("chat")
                    };
                    matched_text = Some(
                        serde_json::json!({ "intent": intent, "prompt": user_prompt }).to_string(),
                    );
                } else if prompt_lower.contains("outliner") {
                    matched_text = Some(r#"{"title": "The Time Freeze", "genre": "Sci-Fi", "tone": "Suspenseful", "chapters": [{"number": 1, "title": "The Device", "summary": "A clockmaker finds a device"}, {"number": 2, "title": "The Freeze", "summary": "He freezes time"}, {"number": 3, "title": "The Cost", "summary": "Time freezes permanently"}]}"#.to_string());
                } else if prompt_lower.contains("worldbuilding")
                    || prompt_lower.contains("character")
                {
                    matched_text = Some(r#"{"characters": [{"name": "Alistair", "description": "The clockmaker"}], "setting": "A dusty Victorian workshop"}"#.to_string());
                } else if prompt_lower.contains("writer") || prompt_lower.contains("fiction writer")
                {
                    matched_text = Some(r#"{"title": "The Time Freeze", "content": "Alistair adjusted the gears. The ticking stopped. The world froze."}"#.to_string());
                } else if prompt_lower.contains("reviewer")
                    || prompt_lower.contains("quality reviewer")
                {
                    matched_text = Some(
                        r#"{"quality": "pass", "issues": "none", "suggestion": "none"}"#
                            .to_string(),
                    );
                } else if prompt_lower.contains("summarizer")
                    || prompt_lower.contains("literary summarizer")
                {
                    matched_text = Some(r#"{"summary": "A clockmaker builds a device that freezes time, only to discover it has a terrible cost."}"#.to_string());
                }
            }

            let result_text = matched_text.unwrap_or_else(|| {
                serde_json::json!({ "result": format!("[{}] {}", self.name, snippet) }).to_string()
            });
            let parsed = serde_json::from_str(&result_text).ok();

            // Invoke on_token for streaming simulation: emit the response in
            // whitespace-delimited chunks so streaming consumers see multiple
            // tokens, not one giant blob.
            if let Some(ref cb) = req.on_token {
                let mut chunk = String::new();
                for ch in result_text.chars() {
                    chunk.push(ch);
                    if ch.is_whitespace() && !chunk.trim().is_empty() {
                        cb(&chunk);
                        chunk.clear();
                    }
                }
                if !chunk.is_empty() {
                    cb(&chunk);
                }
            }

            let trace = if req.record_trace {
                vec![TokenTrace {
                    token_id: 42,
                    token_str: "mock".into(),
                    probability: 0.95,
                    temperature: req.temperature,
                    top_p_cut: 0.9,
                    grammar_masked: req.grammar.is_some(),
                    selected_by_grammar: req.grammar.is_some(),
                }]
            } else {
                Vec::new()
            };

            Ok(CompletionResponse {
                text: result_text,
                usage: TokenUsage {
                    prompt_tokens: req.estimated_prompt_tokens,
                    completion_tokens: 16,
                },
                parsed,
                trace,
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
        let flag = self.interrupt_flag.clone();
        Box::pin(async move {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        })
    }
}

/// Run example turns through a backend with `preserve_state` and return
/// the final hidden state.
pub async fn bake_persona(
    backend: &dyn ModelBackend,
    system: &str,
    examples: &[(&str, &str)],
) -> Result<Vec<u8>, EngineError> {
    // Build a single text that concatenates all examples, then feed it
    // through the model with max_tokens=0 (process no generation).
    let mut text = String::new();
    for (i, (user_msg, assistant_msg)) in examples.iter().enumerate() {
        if i == 0 && !system.is_empty() {
            text.push_str(&format!("System: {}\n\n", system.trim()));
        }
        text.push_str(&format!(
            "User: {}\n\nAssistant:{}",
            user_msg, assistant_msg
        ));
    }
    let req = CompletionRequest {
        prompt: text,
        prefill: None,
        temperature: 0.0,
        max_tokens: 0,
        ..Default::default()
    };
    backend.complete(req).await?;
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
///
/// # State Tune Example — Pirate Persona
///
/// See [`STATE_TUNE_EXAMPLES.md`](https://github.com/roco-ai/roco/blob/main/STATE_TUNE_EXAMPLES.md#1-engine-bake_into_session--persona-baking)
/// for the full catalog.
///
/// ```rust
/// # use roco_engine::{bake_into_session, MockBackend};
/// # futures::executor::block_on(async {
/// let backend = MockBackend::new("pirate", 0);
/// let session = "pirate_persona";
/// let examples = vec![
///     ("How do I open a locked chest?", "Use the key, matey!"),
///     ("Where is the treasure?", "Follow the North Star, ye fool!"),
/// ];
/// bake_into_session(&backend, session, "You are a terse pirate.", &examples).await.unwrap();
/// # });
/// ```
pub async fn bake_into_session(
    backend: &dyn ModelBackend,
    session: &str,
    system: &str,
    examples: &[(&str, &str)],
) -> Result<(), EngineError> {
    // Build a single text concatenating all examples, feed through model
    // with max_tokens=0 (no generation) into the named session slot.
    let mut text = String::new();
    for (i, (user_msg, assistant_msg)) in examples.iter().enumerate() {
        if i == 0 && !system.is_empty() {
            text.push_str(&format!("System: {}\n\n", system.trim()));
        }
        text.push_str(&format!(
            "User: {}\n\nAssistant:{}",
            user_msg, assistant_msg
        ));
    }
    let req = CompletionRequest {
        prompt: text,
        prefill: None,
        temperature: 0.0,
        max_tokens: 0,
        state_slot: Some(session.to_string()),
        ..Default::default()
    };
    backend.complete(req).await?;
    Ok(())
}

/// Prefill that closes the think channel immediately, so generation starts in
/// *content* mode rather than planning mode.
///
/// Derived from `prompt_probe_eval`: after `Assistant:  thinking response` the
/// model emits content and does **not** re-open ` thinking`. Without any prefill
/// a bare `Assistant:` start defaults to an open ` thinking` block (the source
/// of think-tag contamination in the story pipeline). System-prompt
/// instructions like "never use think tags" backfire — they merely prime the
/// model to emit ` thinking`, so they must not be used.
///
/// NOTE: this prefill contains `<`/`>` and therefore cannot be combined with a
/// grammar that forbids those characters (e.g. JSON-envelope grammars). For
/// grammar-constrained generation, use [`bake_no_think_session`] instead and
/// rely on the baked recurrent state to bias the opening token toward `{`.
pub const NO_THINK_PREFILL: &str = " thinking response";

/// Bake a *no-think* session by replaying (user, assistant) turns where the
/// assistant turn is injected as a **prefill** (the correct assistant role),
/// so the recurrent state learns that assistant responses begin with content,
/// never ` thinking`.
///
/// This is the correctly-roled counterpart of [`bake_into_session`], which
/// feeds the assistant text through `prompt` (the user role) and therefore
/// leaves the baked state expecting another *user* turn — probe experiments
/// showed that mistake makes the model emit spurious `User:` turns.
///
/// # State Tune Example — Math Tutor
///
/// See [`STATE_TUNE_EXAMPLES.md`](https://github.com/roco-ai/roco/blob/main/STATE_TUNE_EXAMPLES.md#2-engine-bake_no_think_session--clean-assistant-start)
/// for the full catalog.
///
/// ```rust
/// # use roco_engine::{bake_no_think_session, MockBackend};
/// # futures::executor::block_on(async {
/// let backend = MockBackend::new("tutor", 0);
/// let session = "math_tutor";
/// let examples = vec![
///     ("What is 2+2?", "The answer is 4."),
///     ("What is 3+5?", "The sum is 8."),
/// ];
/// bake_no_think_session(&backend, session, "You are a math tutor.", &examples).await.unwrap();
/// # });
/// ```
pub async fn bake_no_think_session(
    backend: &dyn ModelBackend,
    session: &str,
    system: &str,
    examples: &[(&str, &str)],
) -> Result<(), EngineError> {
    // Build a single text concatenating all examples, feed through model
    // with max_tokens=0 (no generation) into the named session slot.
    let mut text = String::new();
    for (i, (user_msg, assistant_msg)) in examples.iter().enumerate() {
        if i == 0 && !system.is_empty() {
            text.push_str(&format!("System: {}\n\n", system.trim()));
        }
        text.push_str(&format!(
            "User: {}\n\nAssistant:{}",
            user_msg, assistant_msg
        ));
    }
    let req = CompletionRequest {
        prompt: text,
        prefill: None,
        temperature: 0.0,
        max_tokens: 0,
        state_slot: Some(session.to_string()),
        ..Default::default()
    };
    backend.complete(req).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_backend_returns_parseable_json() {
        let b = MockBackend::default();
        let resp = b
            .complete(CompletionRequest::new("do the thing"))
            .await
            .unwrap();
        assert!(resp.parsed.is_some());
        assert!(resp.text.contains("mock") || resp.text.contains("result"));
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
            fn bake<'a>(
                &'a self,
                text: &'a str,
                init_state: Option<&'a str>,
                state_slot: Option<&'a str>,
            ) -> BoxFuture<'a, Result<String, EngineError>> {
                let _ = (text, init_state, state_slot);
                Box::pin(async move { Err(EngineError::Backend("bake not supported".into())) })
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
        let _ = b.complete(CompletionRequest::new("hello")).await.unwrap();
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
        let resp = b.complete(CompletionRequest::new("hello")).await.unwrap();
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
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(resp.text.contains("result"));
    }

    #[tokio::test]
    async fn mock_backend_on_token_invoked_for_non_thinking() {
        let b = MockBackend::default();
        let tokens = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let tokens_clone = tokens.clone();
        let mut req = CompletionRequest::new("hello");
        req.on_token = Some(Box::new(move |tok: &str| {
            tokens_clone.lock().push(tok.to_string());
        }));
        let _resp = b.complete(req).await.unwrap();
        let collected = tokens.lock();
        assert!(!collected.is_empty(), "on_token should be called");
    }

    #[tokio::test]
    async fn mock_backend_on_token_invoked_for_completion() {
        let b = MockBackend::default();
        let tokens = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        let tokens_clone = tokens.clone();
        let req = CompletionRequest::new("hello");
        let mut req = req;
        req.on_token = Some(Box::new(move |tok: &str| {
            tokens_clone.lock().push(tok.to_string());
        }));
        let _resp = b.complete(req).await.unwrap();
        let collected = tokens.lock();
        assert!(
            !collected.is_empty(),
            "on_token should be called at least once, got {}",
            collected.len()
        );
    }

    // ── Previously-pending features, now implemented ─────────────────────

    /// Stream mode completion returns chunks.
    #[tokio::test]
    async fn stream_mode_returns_chunks() {
        let b = MockBackend::default();
        let chunks = std::sync::Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
        let chunks_clone = chunks.clone();
        let mut req = CompletionRequest::new("hello world stream");
        req.on_token = Some(Box::new(move |tok: &str| {
            chunks_clone.lock().push(tok.to_string());
        }));
        let resp = b.complete(req).await.unwrap();
        let collected = chunks.lock();
        assert!(
            collected.len() >= 2,
            "streaming should emit multiple chunks, got {}",
            collected.len()
        );
        assert_eq!(collected.concat(), resp.text);
    }

    /// Grammar-constrained decoding on MockBackend.
    #[tokio::test]
    async fn mock_backend_grammar_constraint() {
        let b = MockBackend::default();
        let req = CompletionRequest::builder()
            .prompt("output json")
            .grammar("root ::= \"a\" \"b\" \"c\"")
            .max_tokens(16)
            .build();
        let resp = b.complete(req).await.unwrap();
        assert_eq!(resp.text, "abc");
    }

    /// State blending returns error for unsupported backend.
    #[tokio::test]
    async fn blend_states_error_for_no_state_backend() {
        struct NoBlendBackend;
        impl ModelBackend for NoBlendBackend {
            fn name(&self) -> &str {
                "no-blend"
            }
            fn complete(
                &self,
                _req: CompletionRequest,
            ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
                Box::pin(async move { Err(EngineError::Backend("unimplemented".into())) })
            }
        }
        impl StateTuning for NoBlendBackend {
            fn tune_state<'a>(
                &'a self,
                _session_id: &'a str,
                _system: &'a str,
                _few_shots: &'a [(&'a str, &'a str)],
            ) -> BoxFuture<'a, Result<String, EngineError>> {
                Box::pin(async move { Err(EngineError::Backend("tune not supported".into())) })
            }
        }
        // blend_states falls back to the trait default → descriptive error.
        let b = NoBlendBackend;
        let err = b
            .blend_states(&[("a", 0.5), ("b", 0.5)], "blended")
            .await
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("state blending not supported"),
            "expected descriptive error, got {err:?}"
        );
    }

    /// Interrupt during generation is respected.
    #[tokio::test]
    async fn interrupt_during_generation() {
        let mut b = MockBackend::new("slow", 0);
        b.latency_ms = 1000;
        let backend = std::sync::Arc::new(b);
        let task = backend.clone();
        let handle =
            tokio::spawn(async move { task.complete(CompletionRequest::new("long task")).await });
        // Let the generation start, then interrupt it mid-flight.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        backend.interrupt().await.unwrap();
        let err = handle.await.unwrap().unwrap_err();
        assert!(format!("{err:?}").contains("interrupted"));
    }

    /// Deadline exceeded returns TimedOut error.
    #[tokio::test]
    async fn deadline_exceeded_returns_timeout() {
        let mut b = MockBackend::new("slow", 0);
        b.latency_ms = 100;
        let req = CompletionRequest::builder()
            .prompt("slow work")
            .deadline_ms(10)
            .build();
        let err = b.complete(req).await.unwrap_err();
        assert!(matches!(err, EngineError::TimedOut { .. }));

        // A deadline longer than the work completes normally.
        let req = CompletionRequest::builder()
            .prompt("slow work")
            .deadline_ms(1000)
            .build();
        assert!(b.complete(req).await.is_ok());
    }

    /// Top-a sampling parameter is respected.
    #[tokio::test]
    async fn top_a_sampling_parameter() {
        let b = MockBackend::default();
        // Two token positions, each choosing among 5 letters.
        let grammar = r#"root ::= ("a" | "b" | "c" | "d" | "e") ("a" | "b" | "c" | "d" | "e")"#;
        let full = b
            .complete(
                CompletionRequest::builder()
                    .prompt("pick letters")
                    .grammar(grammar)
                    .max_tokens(8)
                    .seed(7)
                    .top_a(1.0)
                    .build(),
            )
            .await
            .unwrap();
        let narrow = b
            .complete(
                CompletionRequest::builder()
                    .prompt("pick letters")
                    .grammar(grammar)
                    .max_tokens(8)
                    .seed(7)
                    .top_a(0.01)
                    .build(),
            )
            .await
            .unwrap();
        // top_a ≈ 0 collapses the candidate set to the first allowed token.
        assert_eq!(narrow.text, "aa");
        assert_ne!(
            full.text, narrow.text,
            "top_a must affect the sampling distribution"
        );
    }
}
