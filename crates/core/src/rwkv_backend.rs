//! Real RWKV inference backend using `web-rwkv` (WebGPU).
//!
//! Phase 4 backend — loads a SafeTensors RWKV model from disk, runs inference
//! on the GPU via WebGPU, and returns text completions.
//!
//! Because `web-rwkv`'s `Context` and `State` types are `Send` but **not**
//! `Sync` (they contain `wgpu::Device`), their async methods produce
//! non-`Send` futures.  We work around this by running the entire inference
//! engine on a dedicated OS thread with a single-threaded tokio runtime and
//! `LocalSet`, communicating with the outside world through channels.
//!
//! **Model files** must be in SafeTensors format (converted from PTH via
//! `web-rwkv-converter`).  Set the paths via environment variables:
//!
//! ```bash
//! export RWKV_MODEL=/path/to/model.st    # model weights
//! export RWKV_VOCAB=/path/to/vocab.json  # tokenizer vocabulary
//! ```

use std::any::Any;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

use half::f16;
use memmap2::Mmap;
use safetensors::SafeTensors;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use wgpu::PowerPreference;

use web_rwkv::context::{Context, ContextBuilder, InstanceExt};
use web_rwkv::runtime::infer::{Rnn, RnnInput, RnnInputBatch, RnnOption};
use web_rwkv::runtime::loader::Loader;
use web_rwkv::runtime::model::{
    Bundle, ContextAutoLimits, ModelBuilder, ModelVersion, Quant, State as RwkvState,
};
use web_rwkv::runtime::softmax::softmax_one;
use web_rwkv::runtime::v7;
use web_rwkv::runtime::TokioRuntime;
use web_rwkv::tensor::{TensorCpu, TensorError, TensorInit, TensorShape};
use web_rwkv::tokenizer::Tokenizer;

use crate::engine::{
    BoxFuture, CompletionRequest, CompletionResponse, EngineError, ModelBackend, TokenUsage,
};

// ---------------------------------------------------------------------------
// Sampling
// ---------------------------------------------------------------------------

fn sample_token(probs: &[f32], temperature: f32, top_p: f32) -> u32 {
    if temperature == 0.0 {
        return probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i as u32)
            .unwrap_or(0);
    }
    let mut sorted: Vec<_> = probs.iter().copied().enumerate().collect();
    sorted.sort_unstable_by(|a, b| a.1.total_cmp(&b.1).reverse());
    let mut cum = 0.0f32;
    let mut keep = sorted.len();
    for (_, p) in sorted.iter() {
        cum += p;
        if cum >= top_p {
            break;
        }
        keep -= 1;
    }
    sorted.truncate(keep);
    let sum: f32 = sorted.iter().map(|(_, p)| p.powf(1.0 / temperature)).sum();
    let weighted: Vec<(usize, f32)> = sorted
        .into_iter()
        .map(|(id, p)| (id, p.powf(1.0 / temperature) / sum))
        .collect();
    let r = fastrand::f32();
    let mut cum = 0.0f32;
    for (id, p) in &weighted {
        cum += p;
        if r <= cum {
            return *id as u32;
        }
    }
    weighted.last().map(|(id, _)| *id as u32).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Type-erased state
// ---------------------------------------------------------------------------

enum AnyState {
    V4(Box<dyn Any + Send>),
    V5(Box<dyn Any + Send>),
    V6(Box<dyn Any + Send>),
    V7(Box<dyn Any + Send>),
}

macro_rules! state_back {
    ($s:expr, $ty:ty, $batch:expr) => {{
        let s = ($s).downcast_ref::<$ty>().expect("state downcast");
        s.back($batch).await
    }};
}
macro_rules! state_load {
    ($s:expr, $ty:ty, $tensor:expr, $batch:expr) => {{
        let s = ($s).downcast_ref::<$ty>().expect("state downcast");
        s.load($tensor, $batch)
    }};
}

impl AnyState {
    async fn back(&self, batch: usize) -> Result<TensorCpu<f32>, TensorError> {
        match self {
            AnyState::V4(s) => state_back!(s, web_rwkv::runtime::v4::State, batch),
            AnyState::V5(s) => state_back!(s, web_rwkv::runtime::v5::State, batch),
            AnyState::V6(s) => state_back!(s, web_rwkv::runtime::v6::State, batch),
            AnyState::V7(s) => state_back!(s, v7::State, batch),
        }
    }
    fn load(&self, tensor: TensorCpu<f32>, batch: usize) -> Result<(), TensorError> {
        match self {
            AnyState::V4(s) => state_load!(s, web_rwkv::runtime::v4::State, tensor, batch),
            AnyState::V5(s) => state_load!(s, web_rwkv::runtime::v5::State, tensor, batch),
            AnyState::V6(s) => state_load!(s, web_rwkv::runtime::v6::State, tensor, batch),
            AnyState::V7(s) => state_load!(s, v7::State, tensor, batch),
        }
    }
}

// ---------------------------------------------------------------------------
// Actor — owns non-Send GPU resources, runs on a dedicated thread
// ---------------------------------------------------------------------------

struct RwkvActor {
    context: Context,
    runtime: TokioRuntime<Rnn>,
    state: AnyState,
    tokenizer: Tokenizer,
    token_chunk_size: usize,
    _mmap: Mmap,
}

/// Request sent to the actor through the channel.
struct CompleteReq {
    system: String,
    prompt: String,
    max_tokens: usize,
    temperature: f32,
    reply: oneshot::Sender<Result<(String, TokenUsage), EngineError>>,
}

impl RwkvActor {
    async fn from_env() -> anyhow::Result<Self> {
        // Default: prefer the converted model file (scripts/pth_to_st_converter/convert.py).
        // Fallback: the raw .pth-converted .st file.
        let model_path = env::var("RWKV_MODEL").unwrap_or_else(|_| {
            let dir = std::env::current_dir().unwrap_or_default();
            let candidates = [
                "models/rwkv7-g1g-2.9b-20260526-ctx8192-converted.st",
                "models/rwkv7-g1g-2.9b-20260526-ctx8192.st",
            ];
            for c in &candidates {
                let p = dir.join(c);
                if p.exists() {
                    return p.to_string_lossy().to_string();
                }
            }
            dir.join(candidates[0]).to_string_lossy().to_string()
        });
        let vocab_path = env::var("RWKV_VOCAB").unwrap_or_else(|_| {
            let dir = std::env::current_dir().unwrap_or_default();
            let candidates = [
                "assets/vocab/rwkv_vocab_v20230424.json",
                "models/rwkv_vocab_v20230424.json",
            ];
            for c in &candidates {
                let p = dir.join(c);
                if p.exists() {
                    return p.to_string_lossy().to_string();
                }
            }
            dir.join(candidates[0]).to_string_lossy().to_string()
        });
        let token_chunk_size: usize =
            env::var("RWKV_CHUNK").ok().and_then(|s| s.parse().ok()).unwrap_or(128);

        info!(model_path = %model_path, vocab_path = %vocab_path, "loading RWKV model");

        let vocab_text = tokio::fs::read_to_string(&vocab_path).await?;
        let tokenizer = Tokenizer::new(&vocab_text)?;
        info!("tokenizer loaded");

        let std_file = std::fs::File::open(&model_path)?;
        let mmap = unsafe { Mmap::map(&std_file)? };
        let model = SafeTensors::deserialize(&mmap)?;
        let info = Loader::info(&model)?;
        let version = info.version;
        info!(version = ?version, layers = info.num_layer, vocab = info.num_vocab, emb = info.num_emb, "model info");

        let instance = wgpu::Instance::default();
        let adapter = instance.adapter(PowerPreference::HighPerformance).await?;
        let context = ContextBuilder::new(adapter).auto_limits(&info).build().await?;
        info!(adapter = %context.adapter.get_info().name, "WebGPU context created");

        // Quantization (matches rwkv-harness defaults: no quantization).
        // RWKV_QUANT=N — quantize first N layers with Int8.
        // RWKV_QUANT=nf4=N — quantize first N layers with NF4.
        // Default: no quantization (model weights are streamed through VRAM
        // during load; only embed + head + state stay resident ≈ 700 MB).
        let quant_spec = env::var("RWKV_QUANT").unwrap_or_default();
        let quant_layers: std::collections::HashMap<usize, Quant> = if quant_spec.is_empty() {
            std::collections::HashMap::new()
        } else if let Some(n) = quant_spec.strip_prefix("nf4=") {
            let n = n.parse::<usize>().unwrap_or(0);
            (0..n).map(|l| (l, Quant::NF4)).collect()
        } else {
            let n = quant_spec.parse::<usize>().unwrap_or(0);
            (0..n).map(|l| (l, Quant::Int8)).collect()
        };

        let builder = ModelBuilder::new(&context, model).quant(quant_layers);

        let (runtime, state) = match version {
            ModelVersion::V4 => {
                let m = builder.build_v4().await?;
                let b = web_rwkv::runtime::v4::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let r = TokioRuntime::new(b).await;
                (r, AnyState::V4(Box::new(s)))
            }
            ModelVersion::V5 => {
                let m = builder.build_v5().await?;
                let b = web_rwkv::runtime::v5::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let r = TokioRuntime::new(b).await;
                (r, AnyState::V5(Box::new(s)))
            }
            ModelVersion::V6 => {
                let m = builder.build_v6().await?;
                let b = web_rwkv::runtime::v6::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let r = TokioRuntime::new(b).await;
                (r, AnyState::V6(Box::new(s)))
            }
            ModelVersion::V7 => {
                let m = builder.build_v7().await?;
                let b = v7::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let r = TokioRuntime::new(b).await;
                (r, AnyState::V7(Box::new(s)))
            }
        };

        info!("RWKV runtime initialized");
        Ok(Self {
            context,
            runtime,
            state,
            tokenizer,
            token_chunk_size,
            _mmap: mmap,
        })
    }

    async fn handle_complete(
        &mut self,
        system: &str,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
    ) -> Result<(String, TokenUsage), EngineError> {
        let full = format!("{system}\n\n{prompt}");
        let prompt_tokens = self
            .tokenizer
            .encode(full.as_bytes())
            .map_err(|e| EngineError::Backend(format!("tokenizer encode: {e}")))?;
        let prompt_len = prompt_tokens.len();

        let top_p = if temperature < 0.3 { 0.8 } else if temperature < 0.7 { 0.9 } else { 0.95 };

        let mut inference = RnnInput::new(
            vec![RnnInputBatch::new(prompt_tokens.clone(), RnnOption::Last)],
            self.token_chunk_size,
        );

        // Flush prompt tokens; once all are consumed, sample the first token
        // from the last-prompt-token logits, then break into the generation loop.
        let mut generated = Vec::new();
        let mut text = String::new();
        let mut first_token_sampled = false;

        loop {
            let input = inference.clone();
            let (input, output) = self
                .runtime
                .infer(input)
                .await
                .map_err(|e| EngineError::Backend(format!("RWKV inference: {e}")))?;
            inference = input;

            // Still processing prompt tokens — continue flushing
            if inference.batches[0].tokens.len() > 0 {
                continue;
            }

            // All prompt consumed — use logits to sample first token
            let ot = output[0].0.clone();
            if ot.size() == 0 {
                break; // nothing to generate
            }

            let data = ot.to_vec();
            let shape = ot.shape();
            let cpu = TensorCpu::from_data(shape, data)
                .map_err(|e| EngineError::Backend(format!("tensor creation: {e}")))?;
            let probs = softmax_one(&self.context, cpu)
                .await
                .map_err(|e| EngineError::Backend(format!("softmax: {e}")))?;
            let token = sample_token(probs.data(), temperature, top_p);

            if token == 0 { break; }

            let decoded = self
                .tokenizer
                .decode(&[token])
                .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
            let word = String::from_utf8_lossy(&decoded);

            if word == "\n\n" || word == "\nUser:" || word == "\nHuman:" {
                break;
            }

            text.push_str(&word);
            generated.push(token);
            first_token_sampled = true;
            inference.batches[0].push(token);
            break; // first token sampled — enter generation loop below
        }

        if !first_token_sampled {
            return Ok((text, TokenUsage {
                prompt_tokens: prompt_len,
                completion_tokens: 0,
            }));
        }

        // Generate remaining tokens
        for _ in 1..max_tokens {
            let input = inference.clone();
            let (input, output) = self
                .runtime
                .infer(input)
                .await
                .map_err(|e| EngineError::Backend(format!("RWKV inference: {e}")))?;
            inference = input;

            let ot = output[0].0.clone();
            if ot.size() == 0 { break; }

            let data = ot.to_vec();
            let shape = ot.shape();
            let cpu = TensorCpu::from_data(shape, data)
                .map_err(|e| EngineError::Backend(format!("tensor creation: {e}")))?;
            let probs = softmax_one(&self.context, cpu)
                .await
                .map_err(|e| EngineError::Backend(format!("softmax: {e}")))?;
            let token = sample_token(probs.data(), temperature, top_p);

            if token == 0 { break; }

            let decoded = self
                .tokenizer
                .decode(&[token])
                .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
            let word = String::from_utf8_lossy(&decoded);

            if word == "\n\n" || word == "\nUser:" || word == "\nHuman:" {
                break;
            }

            text.push_str(&word);
            generated.push(token);
            inference.batches[0] = RnnInputBatch::new(vec![token], RnnOption::Last);
        }

        let text = if generated.is_empty() {
            String::new()
        } else {
            let decoded = self
                .tokenizer
                .decode(&generated)
                .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
            String::from_utf8_lossy(&decoded).to_string()
        };

        Ok((
            text,
            TokenUsage {
                prompt_tokens: prompt_len,
                completion_tokens: generated.len(),
            },
        ))
    }

    /// Run the actor message loop on the local set.
    async fn run(mut self, mut rx: mpsc::Receiver<CompleteReq>) {
        while let Some(req) = rx.recv().await {
            let result = self
                .handle_complete(&req.system, &req.prompt, req.max_tokens, req.temperature)
                .await;
            let _ = req.reply.send(result);
        }
    }
}

// ---------------------------------------------------------------------------
// Backend — thread-safe handle to the actor thread
// ---------------------------------------------------------------------------

pub struct RwkvBackend {
    tx: mpsc::Sender<CompleteReq>,
    name: String,
}

impl RwkvBackend {
    /// Build from environment variables.
    ///
    /// Spawns a dedicated OS thread that owns all non-Send GPU resources and
    /// runs a single-threaded tokio runtime with a `LocalSet` (required because
    /// `web-rwkv`'s async methods produce non-`Send` futures).
    pub fn from_env() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<CompleteReq>(4);

        std::thread::Builder::new()
            .name("rwkv-actor".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build rwkv runtime");
                let local = tokio::task::LocalSet::new();

                local.spawn_local(async move {
                    match RwkvActor::from_env().await {
                        Ok(actor) => {
                            info!("RWKV actor ready on dedicated thread");
                            actor.run(rx).await;
                        }
                        Err(e) => warn!("RWKV actor failed to initialize: {e}"),
                    }
                });

                // Drive the local set until the actor thread is done.
                let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
                let _ = local.block_on(&rt, rx);
            })
            .expect("failed to spawn rwkv actor thread");

        Ok(Self {
            tx,
            name: "rwkv".to_string(),
        })
    }

    /// Build from explicit model/vocab paths.
    pub fn from_paths(
        model_path: impl Into<PathBuf>,
        vocab_path: impl Into<PathBuf>,
    ) -> anyhow::Result<Self> {
        let mp = model_path.into();
        let vp = vocab_path.into();
        let prev_m = env::var("RWKV_MODEL").ok();
        let prev_v = env::var("RWKV_VOCAB").ok();
        env::set_var("RWKV_MODEL", mp.to_string_lossy().as_ref());
        env::set_var("RWKV_VOCAB", vp.to_string_lossy().as_ref());
        let result = Self::from_env();
        match prev_m {
            Some(v) => env::set_var("RWKV_MODEL", v),
            None => env::remove_var("RWKV_MODEL"),
        }
        match prev_v {
            Some(v) => env::set_var("RWKV_VOCAB", v),
            None => env::remove_var("RWKV_VOCAB"),
        }
        result
    }
}

impl ModelBackend for RwkvBackend {
    fn name(&self) -> &str {
        &self.name
    }
    fn complete(
        &self,
        req: CompletionRequest,
    ) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        let tx = self.tx.clone();
        Box::pin(async move {
            let started = Instant::now();
            let (reply_tx, reply_rx) = oneshot::channel();
            tx.send(CompleteReq {
                system: req.system,
                prompt: req.prompt,
                max_tokens: req.max_tokens,
                temperature: req.temperature,
                reply: reply_tx,
            })
            .await
            .map_err(|e| EngineError::Backend(format!("rwkv channel send: {e}")))?;

            let (text, usage) = reply_rx
                .await
                .map_err(|e| EngineError::Backend(format!("rwkv channel recv: {e}")))?
                .map_err(|e| EngineError::Backend(format!("rwkv actor error: {e}")))?;

            info!(
                ms = started.elapsed().as_millis(),
                prompt_tokens = usage.prompt_tokens,
                completion_tokens = usage.completion_tokens,
                snippet = %text.chars().take(200).collect::<String>(),
                "rwkv complete"
            );

            let parsed = serde_json::from_str(&text).ok();
            Ok(CompletionResponse {
                text,
                usage,
                parsed,
            })
        })
    }
}
