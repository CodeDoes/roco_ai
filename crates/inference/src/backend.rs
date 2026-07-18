//! [`RwkvBackend`] — thread-safe handle to the dedicated actor thread.
//!
//! Spawns a dedicated OS thread that owns all non-Send GPU resources and
//! runs a single-threaded tokio runtime with a `LocalSet`. Communicates
//! via channels.

use std::path::PathBuf;
use std::time::Instant;

use futures::future::BoxFuture;
use roco_engine::{CompletionRequest, CompletionResponse, EngineError, ModelBackend};
use tokio::sync::mpsc;
use tracing::info;

use crate::actor::{ActorMessage, BlendReq, CompleteReq, RwkvActor};
use tokio::sync::oneshot;

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
        let default_deadline_ms = std::env::var("RWKV_DEADLINE_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let (tx, rx) = mpsc::channel::<ActorMessage>(4);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<std::result::Result<(), String>>();

        let actor_thread = std::thread::Builder::new()
            .name("rwkv-actor".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all().build().expect("failed to build rwkv runtime");
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

        futures::executor::block_on(async {
            match ready_rx.await {
                Ok(Ok(())) => Ok::<_, anyhow::Error>(()),
                Ok(Err(msg)) => Err(anyhow::anyhow!("RWKV backend init failed: {msg}")),
                Err(_) => Err(anyhow::anyhow!("RWKV actor thread died before init")),
            }
        })?;

        Ok(Self { tx: Some(tx), actor_thread: Some(actor_thread), name: "rwkv".to_string(), default_deadline_ms })
    }

    /// Build from explicit model/vocab paths.
    pub fn from_paths(model_path: impl Into<PathBuf>, vocab_path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let mp = model_path.into();
        let vp = vocab_path.into();
        let prev_m = std::env::var("RWKV_MODEL").ok();
        let prev_v = std::env::var("RWKV_VOCAB").ok();
        std::env::set_var("RWKV_MODEL", mp.to_string_lossy().as_ref());
        std::env::set_var("RWKV_VOCAB", vp.to_string_lossy().as_ref());
        let result = Self::from_env();
        match prev_m { Some(v) => std::env::set_var("RWKV_MODEL", v), None => std::env::remove_var("RWKV_MODEL") }
        match prev_v { Some(v) => std::env::set_var("RWKV_VOCAB", v), None => std::env::remove_var("RWKV_VOCAB") }
        result
    }
}

impl RwkvBackend {
    /// Get the model's vocabulary bytes (token_id → raw bytes).
    /// Used by the application layer to create `BnfMask` instances.
    #[cfg(feature = "grammar")]
    pub fn vocab_bytes(&self) -> Result<Vec<Vec<u8>>, EngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let tx = self.tx.clone().ok_or_else(||
            EngineError::Backend("backend shut down".into())
        )?;
        futures::executor::block_on(async {
            tx.send(ActorMessage::GetVocabBytes(reply_tx)).await
                .map_err(|e| EngineError::Backend(format!("get_vocab_bytes send: {e}")))?;
            reply_rx.await
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
        let tx = self.tx.clone().ok_or_else(||
            EngineError::Backend("backend shut down".into())
        )?;
        futures::executor::block_on(async {
            tx.send(ActorMessage::BlendStates(BlendReq {
                session_a: session_a.to_string(),
                session_b: session_b.to_string(),
                alpha,
                output_session: output_session.to_string(),
                reply: reply_tx,
            })).await
                .map_err(|e| EngineError::Backend(format!("blend_states send: {e}")))?;
            reply_rx.await
                .map_err(|e| EngineError::Backend(format!("blend_states recv: {e}")))?
        })
    }
}

impl ModelBackend for RwkvBackend {
    fn name(&self) -> &str { &self.name }

    fn vocab_bytes(&self) -> Option<Vec<Vec<u8>>> {
        self.vocab_bytes().ok()
    }

    fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        let tx = self.tx.clone().expect("rwkv backend already shut down (channel closed)");
        Box::pin(async move {
            let started = Instant::now();
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

            tx.send(CompleteReq {
                system: req.system, prompt: req.prompt, prefill: req.prefill,
                max_tokens: req.max_tokens,
                temperature: req.temperature,
                top_a: req.top_a,
                grammar: req.grammar,
                bnf_mask: req.bnf_mask,
                reply: reply_tx,
                preserve_state: req.preserve_state, on_token: req.on_token,
                session: req.session,
                deadline_ms: req.deadline_ms,
            }.into()).await
                .map_err(|e| EngineError::Backend(format!("rwkv channel send: {e}")))?;

            // Wall-clock timeout on the entire generation (including prompt
            // processing). If the deadline is exceeded we send a Cancel to the
            // actor (cooperative interrupt, lands within one chunk thanks to
            // the rx.try_recv drain in handle_complete) and return TimedOut.
            // Priority: per-request deadline > default > none (0 = no deadline).
            let effective_deadline_ms = if req.deadline_ms > 0 { req.deadline_ms } else { self.default_deadline_ms };
            let (text, usage) = if effective_deadline_ms > 0 {
                let timeout = tokio::time::Duration::from_millis(effective_deadline_ms);
                match tokio::time::timeout(timeout, reply_rx).await {
                    Ok(Ok(inner)) => inner
                        .map_err(|e| EngineError::Backend(format!("rwkv actor error: {e}")))?,
                    Ok(Err(e)) => return Err(EngineError::Backend(format!("rwkv channel recv: {e}"))),
                    Err(_elapsed) => {
                        let _ = tx.send(ActorMessage::Cancel).await;
                        return Err(EngineError::TimedOut { ms: effective_deadline_ms });
                    }
                }
            } else {
                reply_rx.await
                    .map_err(|e| EngineError::Backend(format!("rwkv channel recv: {e}")))?
                    .map_err(|e| EngineError::Backend(format!("rwkv actor error: {e}")))?
            };

            info!(ms = started.elapsed().as_millis(), prompt_tokens = usage.prompt_tokens,
                completion_tokens = usage.completion_tokens,
                snippet = %text.chars().take(200).collect::<String>(), "rwkv complete");

            let parsed = serde_json::from_str(&text).ok();
            Ok(CompletionResponse { text, usage, parsed, think_trace: None })
        })
    }

    fn interrupt(&self) -> BoxFuture<'_, Result<(), EngineError>> {
        let tx = self.tx.clone().expect("rwkv backend already shut down");
        Box::pin(async move {
            tx.send(ActorMessage::Cancel).await
                .map_err(|e| EngineError::Backend(format!("rwkv interrupt send: {e}")))?;
            Ok(())
        })
    }

    fn save_state(&self) -> BoxFuture<'_, Result<Vec<u8>, EngineError>> {
        let tx = self.tx.clone().expect("rwkv backend already shut down (channel closed)");
        Box::pin(async move {
            let (rtx, rrx) = tokio::sync::oneshot::channel();
            tx.send(ActorMessage::SaveState(rtx)).await
                .map_err(|e| EngineError::Backend(format!("rwkv save_state send: {e}")))?;
            rrx.await
                .map_err(|e| EngineError::Backend(format!("rwkv save_state recv: {e}")))?
        })
    }

    fn load_state(&self, state: Vec<u8>) -> BoxFuture<'_, Result<(), EngineError>> {
        let tx = self.tx.clone().expect("rwkv backend already shut down (channel closed)");
        Box::pin(async move {
            let (rtx, rrx) = tokio::sync::oneshot::channel();
            tx.send(ActorMessage::LoadState(state, rtx)).await
                .map_err(|e| EngineError::Backend(format!("rwkv load_state send: {e}")))?;
            rrx.await
                .map_err(|e| EngineError::Backend(format!("rwkv load_state recv: {e}")))?
        })
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
