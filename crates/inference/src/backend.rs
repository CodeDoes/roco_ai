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

use crate::actor::{ActorMessage, CompleteReq, RwkvActor};
use tokio::sync::oneshot;

/// Thread-safe handle to the RWKV inference actor.
pub struct RwkvBackend {
    tx: Option<mpsc::Sender<ActorMessage>>,
    actor_thread: Option<std::thread::JoinHandle<()>>,
    name: String,
}

impl RwkvBackend {
    /// Build from environment variables.
    ///
    /// Spawns a dedicated OS thread owning all non-Send GPU resources.
    /// Blocks until the model is fully loaded.
    pub fn from_env() -> anyhow::Result<Self> {
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

        Ok(Self { tx: Some(tx), actor_thread: Some(actor_thread), name: "rwkv".to_string() })
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
}

impl ModelBackend for RwkvBackend {
    fn name(&self) -> &str { &self.name }

    fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        let tx = self.tx.clone().expect("rwkv backend already shut down (channel closed)");
        Box::pin(async move {
            let started = Instant::now();
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

            tx.send(CompleteReq {
                system: req.system, prompt: req.prompt, max_tokens: req.max_tokens,
                temperature: req.temperature, grammar: req.grammar,
                bnf_mask: req.bnf_mask,
                reply: reply_tx,
                preserve_state: req.preserve_state, on_token: req.on_token,
                session: req.session,
            }.into()).await
                .map_err(|e| EngineError::Backend(format!("rwkv channel send: {e}")))?;

            let (text, usage) = reply_rx.await
                .map_err(|e| EngineError::Backend(format!("rwkv channel recv: {e}")))?
                .map_err(|e| EngineError::Backend(format!("rwkv actor error: {e}")))?;

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
}

impl Drop for RwkvBackend {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(handle) = self.actor_thread.take() {
            let _ = handle.join();
        }
    }
}
