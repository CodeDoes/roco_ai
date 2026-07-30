//! [`RwkvBackend`] — thread-safe handle to the dedicated actor thread.
//!
//! Spawns a dedicated OS thread that owns all non-Send GPU resources and
//! runs a single-threaded tokio runtime with a `LocalSet`. Communicates
//! via channels.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use roco_engine::{CompletionRequest, CompletionResponse, EngineError, ModelBackend, StateTuning};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tracing::info;

use crate::actor::{ActorMessage, BlendReq, CompleteReq, RwkvActor};

/// Thread-safe handle to the RWKV inference actor.
pub struct RwkvBackend {
    tx: Option<mpsc::Sender<ActorMessage>>,
    actor_thread: Option<std::thread::JoinHandle<()>>,
    name: String,
    /// Default wall-clock deadline for completions (ms). 0 = no deadline.
    /// Can be overridden per-request via CompletionRequest::deadline_ms.
    default_deadline_ms: u64,
}

impl RwkvBackend {
    /// Build from environment variables.
    ///
    /// Spawns a dedicated OS thread owning all non-Send GPU resources.
    /// Blocks until the model is fully loaded.
    pub fn from_env() -> anyhow::Result<Self> {
        let default_deadline_ms = std::env::var("RWKV_DEADLINE_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let (tx, rx) = mpsc::channel::<ActorMessage>(4);
        // Use std::sync::mpsc (not tokio::sync::oneshot) for the ready
        // signal so the main thread can block with a timeout without needing
        // a tokio runtime. This avoids the cross-executor fragility of
        // futures::executor::block_on driving a tokio oneshot.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        let actor_thread = std::thread::Builder::new()
            .name("rwkv-actor".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build rwkv runtime");
                let local = tokio::task::LocalSet::new();

                let actor_handle = local.spawn_local(async move {
                    match RwkvActor::from_env().await {
                        Ok(actor) => {
                            info!("RWKV actor ready on dedicated thread");
                            let _ = ready_tx.send(Ok(()));
                            actor.run(rx).await;
                        }
                        Err(e) => {
                            let _ = ready_tx.send(Err(format!("{e}")));
                        }
                    }
                });

                let _ = local.block_on(&rt, actor_handle);
            })
            .expect("failed to spawn rwkv actor thread");

        // Block with a timeout so a hanging GPU init doesn't freeze the
        // process forever. Debug builds are ~4x slower, so use 300s there.
        // Default 120s for release, overridable via RWKV_BACKEND_TIMEOUT.
        let default_timeout: u64 = if cfg!(debug_assertions) { 300 } else { 120 };
        let timeout_secs: u64 = std::env::var("RWKV_BACKEND_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default_timeout);
        let timeout = Duration::from_secs(timeout_secs);
        match ready_rx.recv_timeout(timeout) {
            Ok(Ok(())) => {}
            Ok(Err(msg)) => {
                anyhow::bail!("RWKV backend init failed: {msg}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                anyhow::bail!(
                    "RWKV backend init timed out after {timeout_secs}s. \
                     The model file may be too large, the GPU may be busy, \
                     or Vulkan drivers may be missing. \
                     Set RWKV_BACKEND_TIMEOUT for a longer wait, \
                     or check GPU setup with: roco gpu-check"
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!(
                    "RWKV actor thread died before initialization completed. \
                     Check the logs above for panic details."
                );
            }
        }

        Ok(Self {
            tx: Some(tx),
            actor_thread: Some(actor_thread),
            name: "rwkv".to_string(),
            default_deadline_ms,
        })
    }

    /// Build from explicit model/vocab paths.
    pub fn from_paths(
        model_path: impl Into<PathBuf>,
        vocab_path: impl Into<PathBuf>,
    ) -> anyhow::Result<Self> {
        let mp = model_path.into();
        let vp = vocab_path.into();
        let prev_m = std::env::var("RWKV_MODEL").ok();
        let prev_v = std::env::var("RWKV_VOCAB").ok();
        std::env::set_var("RWKV_MODEL", mp.to_string_lossy().as_ref());
        std::env::set_var("RWKV_VOCAB", vp.to_string_lossy().as_ref());
        let result = Self::from_env();
        match prev_m {
            Some(v) => std::env::set_var("RWKV_MODEL", v),
            None => std::env::remove_var("RWKV_MODEL"),
        }
        match prev_v {
            Some(v) => std::env::set_var("RWKV_VOCAB", v),
            None => std::env::remove_var("RWKV_VOCAB"),
        }
        result
    }
}

impl RwkvBackend {
    /// Get the model's vocabulary bytes (token_id → raw bytes).
    /// Used by the application layer to create `BnfMask` instances.
    pub fn vocab_bytes(&self) -> Result<Vec<Vec<u8>>, EngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let tx = self
            .tx
            .clone()
            .ok_or_else(|| EngineError::Backend("backend shut down".into()))?;
        futures::executor::block_on(async {
            tx.send(ActorMessage::GetVocabBytes(reply_tx))
                .await
                .map_err(|e| EngineError::Backend(format!("get_vocab_bytes send: {e}")))?;
            reply_rx
                .await
                .map_err(|e| EngineError::Backend(format!("get_vocab_bytes recv: {e}")))
        })
    }

    /// Blend two session states element-wise and store as a new session.
    /// output = alpha * session_a + (1-alpha) * session_b
    pub fn blend_states(
        &self,
        session_a: &str,
        session_b: &str,
        alpha: f32,
        output_session: &str,
    ) -> Result<(), EngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let tx = self
            .tx
            .clone()
            .ok_or_else(|| EngineError::Backend("backend shut down".into()))?;
        futures::executor::block_on(async {
            tx.send(ActorMessage::BlendStates(BlendReq {
                session_a: session_a.to_string(),
                session_b: session_b.to_string(),
                alpha,
                output_session: output_session.to_string(),
                reply: reply_tx,
            }))
            .await
            .map_err(|e| EngineError::Backend(format!("blend_states send: {e}")))?;
            reply_rx
                .await
                .map_err(|e| EngineError::Backend(format!("blend_states recv: {e}")))?
        })
    }
}

impl ModelBackend for RwkvBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn vocab_bytes(&self) -> Option<Vec<Vec<u8>>> {
        self.vocab_bytes().ok()
    }

    fn complete(
        &self,
        req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        let tx = self
            .tx
            .clone()
            .expect("rwkv backend already shut down (channel closed)");
        Box::pin(async move {
            // Grammar handling: this crate does NOT build BnfMask instances
            // (doing so would pull kbnf types into web-rwkv's compilation
            // unit, triggering E0275). Callers must either:
            //   (a) send `req.grammar` as a string over HTTP to `roco-inferd`,
            //       whose server route builds the mask via `create_bnf_mask`,
            //       or
            //   (b) pass a pre-built `req.bnf_mask` (Box<dyn BnfMask>) built
            //       in a crate that already depends on `roco-bnf-engine`.
            //
            // Any `req.grammar` string set here is forwarded to the actor
            // for logging only; the actor ignores it for masking.
            let started = Instant::now();
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

            tx.send(
                CompleteReq {
                    prompt: req.prompt,
                    prefill: req.prefill,
                    max_tokens: req.max_tokens,
                    temperature: req.temperature,
                    top_a: req.top_a,
                    grammar: req.grammar,
                    bnf_mask: req.bnf_mask,
                    reply: reply_tx,
                    on_token: req.on_token,
                    init_state: req.init_state,
                    state_slot: req.state_slot,
                    deadline_ms: req.deadline_ms,
                    seed: req.seed.or_else(|| {
                        std::env::var("RWKV_DETERMINISTIC_SEED")
                            .ok()
                            .and_then(|s| s.parse::<u64>().ok())
                    }),
                    record_trace: req.record_trace,
                }
                .into(),
            )
            .await
            .map_err(|e| EngineError::Backend(format!("rwkv channel send: {e}")))?;

            // Wall-clock timeout on the entire generation (including prompt
            // processing). If the deadline is exceeded we send a Cancel to the
            // actor (cooperative interrupt, lands within one chunk thanks to
            // the rx.try_recv drain in handle_complete) and return TimedOut.
            // Priority: per-request deadline > default > none (0 = no deadline).
            let effective_deadline_ms = if req.deadline_ms > 0 {
                req.deadline_ms
            } else {
                self.default_deadline_ms
            };
            let (text, usage, trace) = if effective_deadline_ms > 0 {
                let timeout = tokio::time::Duration::from_millis(effective_deadline_ms);
                match tokio::time::timeout(timeout, reply_rx).await {
                    Ok(Ok(inner)) => {
                        inner.map_err(|e| EngineError::Backend(format!("rwkv actor error: {e}")))?
                    }
                    Ok(Err(e)) => {
                        return Err(EngineError::Backend(format!("rwkv channel recv: {e}")))
                    }
                    Err(_elapsed) => {
                        let _ = tx.send(ActorMessage::Cancel).await;
                        return Err(EngineError::TimedOut {
                            ms: effective_deadline_ms,
                        });
                    }
                }
            } else {
                reply_rx
                    .await
                    .map_err(|e| EngineError::Backend(format!("rwkv channel recv: {e}")))?
                    .map_err(|e| EngineError::Backend(format!("rwkv actor error: {e}")))?
            };

            info!(ms = started.elapsed().as_millis(), prompt_tokens = usage.prompt_tokens,
                completion_tokens = usage.completion_tokens,
                trace_len = trace.len(),
                snippet = %text.chars().take(200).collect::<String>(), "rwkv complete");

            let parsed = serde_json::from_str(&text).ok();
            Ok(CompletionResponse {
                text,
                usage,
                parsed,
                                trace,
            })
        })
    }

    fn interrupt(&self) -> BoxFuture<'_, Result<(), EngineError>> {
        let tx = self.tx.clone().expect("rwkv backend already shut down");
        Box::pin(async move {
            tx.send(ActorMessage::Cancel)
                .await
                .map_err(|e| EngineError::Backend(format!("rwkv interrupt send: {e}")))?;
            Ok(())
        })
    }

    fn bake<'a>(
        &'a self,
        text: &'a str,
        init_state: Option<&'a str>,
        state_slot: Option<&'a str>,
    ) -> BoxFuture<'a, Result<String, EngineError>> {
        let tx = self
            .tx
            .clone()
            .expect("rwkv backend already shut down (channel closed)");
        let text = text.to_string();
        let init_state = init_state.map(|s| s.to_string());
        let state_slot = state_slot.map(|s| s.to_string());
        Box::pin(async move {
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            tx.send(
                ActorMessage::Bake {
                    init_state,
                    text,
                    state_slot,
                    reply: reply_tx,
                }
            )
            .await
            .map_err(|e| EngineError::Backend(format!("rwkv channel send: {e}")))?;
            reply_rx
                .await
                .map_err(|e| EngineError::Backend(format!("rwkv channel recv: {e}")))?
                .map_err(|e| EngineError::Backend(format!("rwkv actor error: {e}")))?;
            Ok(state_slot.unwrap_or_default())
        })
    }

    fn save_state(&self) -> BoxFuture<'_, Result<Vec<u8>, EngineError>> {
        let tx = self
            .tx
            .clone()
            .expect("rwkv backend already shut down (channel closed)");
        Box::pin(async move {
            let (rtx, rrx) = tokio::sync::oneshot::channel();
            tx.send(ActorMessage::SaveState(rtx))
                .await
                .map_err(|e| EngineError::Backend(format!("rwkv save_state send: {e}")))?;
            rrx.await
                .map_err(|e| EngineError::Backend(format!("rwkv save_state recv: {e}")))?
        })
    }

    fn load_state(&self, state: Vec<u8>) -> BoxFuture<'_, Result<(), EngineError>> {
        let tx = self
            .tx
            .clone()
            .expect("rwkv backend already shut down (channel closed)");
        Box::pin(async move {
            let (rtx, rrx) = tokio::sync::oneshot::channel();
            tx.send(ActorMessage::LoadState(state, rtx))
                .await
                .map_err(|e| EngineError::Backend(format!("rwkv load_state send: {e}")))?;
            rrx.await
                .map_err(|e| EngineError::Backend(format!("rwkv load_state recv: {e}")))?
        })
    }

}

impl StateTuning for RwkvBackend {
    fn tune_state<'a>(
        &'a self,
        session_id: &'a str,
        _system: &'a str,
        few_shots: &'a [(&'a str, &'a str)],
    ) -> BoxFuture<'a, Result<String, EngineError>> {
        let tx = self
            .tx
            .clone()
            .expect("rwkv backend already shut down (channel closed)");
        let session_id = session_id.to_string();
        let few_shots = few_shots
            .iter()
            .map(|(u, a)| (u.to_string(), a.to_string()))
            .collect::<Vec<_>>();
        Box::pin(async move {
            for (_i, (user_msg, assistant_msg)) in few_shots.iter().enumerate() {
                let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                // Format the example the same way the gateway will format
                // real prompts: System: ...\n\nUser: ...\n\nAssistant:{response}
                let sys_text = if _system.is_empty() { String::new() } else {
                    format!("System: {}\n\n", _system.trim())
                };
                let text = format!("{}User: {}\n\nAssistant:{}", sys_text, user_msg, assistant_msg);
                tx.send(
                    ActorMessage::Bake {
                        init_state: Some(session_id.clone()),
                        text,
                        state_slot: session_id.clone(),
                        reply: reply_tx,
                    }
                )
                .await
                .map_err(|e| EngineError::Backend(format!("rwkv channel send: {e}")))?;

                let _ = reply_rx
                    .await
                    .map_err(|e| EngineError::Backend(format!("rwkv channel recv: {e}")))?
                    .map_err(|e| EngineError::Backend(format!("rwkv actor error: {e}")))?;
            }

            Ok(session_id)
        })
    }

    fn blend_states<'a>(
        &'a self,
        session_a: &'a str,
        session_b: &'a str,
        alpha: f32,
        output_session: &'a str,
    ) -> BoxFuture<'a, Result<(), EngineError>> {
        let res = self.blend_states(session_a, session_b, alpha, output_session);
        Box::pin(
            async move { res.map_err(|e| EngineError::Backend(format!("rwkv blend_states: {e}"))) },
        )
    }
}

impl RwkvBackend {
    /// Gracefully shut down the backend: stop accepting new requests,
    /// wait for any in-flight generation to finish (or be cancelled),
    /// then join the actor thread and release GPU resources.
    ///
    /// This is idempotent and safe to call multiple times. After the
    /// first call, subsequent calls are no-ops.
    pub async fn shutdown(&mut self) {
        // 1) Close the request channel so no new CompleteReq can be sent.
        self.tx.take();
        // 2) Join the actor thread (it will exit once its mailbox is closed).
        if let Some(handle) = self.actor_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RwkvBackend {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(handle) = self.actor_thread.take() {
            let _ = handle.join();
        }
    }
}
