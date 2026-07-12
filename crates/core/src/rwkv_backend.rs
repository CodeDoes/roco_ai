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
//!
//! ## ⚠ Debug-mode GPU hang
//!
//! `build_v7()` hangs indefinitely in **debug** builds on some GPU/driver
//! combinations (AMD RADV RENOIR iGPU, NVIDIA RTX 2050 discrete GPU).  Root
//! cause: wgpu validation layers enabled in debug + unoptimized CPU code cause
//! GPU submissions to be spaced far enough apart that the driver kills the GPU
//! context (TDR = Timeout Detection & Recovery).  The `device.poll()` call
//! inside `build_v7` never returns because the context was lost.
//!
//! **Release builds work fine** — wgpu validation is disabled and CPU-heavy
//! tensor processing is optimized, so GPU submissions are fast enough to stay
//! within driver timeouts.  Always build with `--release` for GPU inference.
//!
//! The `build_v7_with_timeout` wrapper below ensures that even in debug mode,
//! the process doesn't hang forever — it returns an error after 300s.
//!

use std::any::Any;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

use half::f16;
use safetensors::SafeTensors;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use web_rwkv::context::{Context, ContextBuilder};
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
// Type-erased state (reserved for future state persistence across turns)
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
    #[allow(dead_code)]
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
    /// Saved initial (blank) state tensor. Loaded before each `complete()` call
    /// so no state leaks between independent inference requests.
    initial_state: TensorCpu<f32>,
    tokenizer: Tokenizer,
    token_chunk_size: usize,
    /// Keep model bytes alive as long as the actor exists.
    /// SafeTensors borrows from this, and ModelBuilder consumes SafeTensors
    /// before it returns, so this is just a safety net.
    #[allow(dead_code)]
    _model_data: Vec<u8>,
}

/// Request sent to the actor through the channel.
struct CompleteReq {
    system: String,
    prompt: String,
    max_tokens: usize,
    temperature: f32,
    reply: oneshot::Sender<Result<(String, TokenUsage), EngineError>>,
}

/// Get the path to the pipeline cache file for a given model.
fn get_pipeline_cache_path(model_path: &str) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_path.hash(&mut hasher);
    let hash = hasher.finish();
    std::path::PathBuf::from("/tmp/roco-pipeline-cache")
        .join(format!("{:016x}.bin", hash))
}

/// Get the directory for cached quantized Int8 weights.
fn get_quant_cache_dir(model_path: &str) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_path.hash(&mut hasher);
    let hash = hasher.finish();
    std::path::PathBuf::from("/tmp/roco-quant-cache")
        .join(format!("{:016x}", hash))
}

/// Default quantization: Int8 for all layers.
///
/// The 2.9B RWKV model (5.5 GB FP16) does NOT fit in 4 GB VRAM unquantized.
/// Int8 halves the size (~2.75 GB resident + 0.3 GB head = ~3 GB).
/// NF4 requires cooperative matrix support (most GPUs lack this).
///
/// Override with `RWKV_QUANT`: "none", "nf4=N", or a number (Int8 N layers).
fn default_quant() -> std::collections::HashMap<usize, Quant> {
    (0..32).map(|l| (l, Quant::Int8)).collect()
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

        // Use `std::fs::read` (not Mmap) — matches the proven rwkv-harness approach.
        // Mmap also works but Vec<u8> avoids any alignment edge cases in debug builds.
        let model_data = std::fs::read(&model_path)?;
        let model = SafeTensors::deserialize(&model_data)?;
        let info = Loader::info(&model)?;
        let version = info.version;
        info!(version = ?version, layers = info.num_layer, vocab = info.num_vocab, emb = info.num_emb, "model info");

        // --- GPU enumeration & capability detection ---
        let instance = wgpu::Instance::default();
        let all_adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;

        // Score each adapter.
        // Cooperative matrix support is REQUIRED for build_v*() to not hang.
        // Without it, the model weight upload stalls indefinitely on some drivers.
        // Score: coop_matrix (100) > device_type (30/20/15/10/5) > buffer_size bonus.
        let mut scored: Vec<_> = all_adapters
            .into_iter()
            .map(|a| {
                let i = a.get_info();
                let coop = a.features().contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);
                let max_buf_mb = a.limits().max_buffer_size / (1024 * 1024);
                let type_score = match i.device_type {
                    wgpu::DeviceType::DiscreteGpu => 30,
                    wgpu::DeviceType::IntegratedGpu => 20,
                    wgpu::DeviceType::VirtualGpu => 15,
                    wgpu::DeviceType::Other => 10,
                    wgpu::DeviceType::Cpu => 5,
                };
                // Coop matrix is the deciding factor — worth more than device type.
                let coop_bonus = if coop { 100 } else { 0 };
                info!(
                    "  [{}] {} | type={:?} | coop_matrix={} | max_buffer={}MB | backend={:?}",
                    if coop { "✓" } else { "✗" },
                    i.name, i.device_type, coop, max_buf_mb, i.backend
                );
                (a, coop_bonus + type_score + (max_buf_mb / 512) as u32)
            })
            .collect();
        scored.sort_by_key(|&(_, s)| std::cmp::Reverse(s));

        // --- Try each adapter in score order until context creation succeeds ---
        // Cooperative matrix adapters are tried first (score includes +100 bonus).
        let adapter_name_filter = env::var("RWKV_ADAPTER").ok();
        let adapter_count = scored.len();

        let mut context: Option<Context> = None;
        let mut gpu_coop = false;
        let mut gpu_max_mb = 0u64;
        let mut gpu_info_name = String::new();
        let mut gpu_device_type = wgpu::DeviceType::Other;

        for (adapter, _score) in scored {
            let ainfo = adapter.get_info();
            let coop = adapter.features().contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);
            let max_mb = adapter.limits().max_buffer_size / (1024 * 1024);

            // If user requested a specific adapter, skip others.
            if let Some(ref filter) = adapter_name_filter {
                if !ainfo.name.to_lowercase().contains(&filter.to_lowercase()) {
                    continue;
                }
            }

            info!("trying adapter: '{}' (type={:?}, coop={}, {}MB)",
                ainfo.name, ainfo.device_type, coop, max_mb);

            // Try to load previously cached pipeline shaders (speeds up
            // subsequent loads by ~10-15s by skipping WGPU shader recompilation).
            let cache_path = get_pipeline_cache_path(&model_path);
            let cached_pipelines = std::fs::read(&cache_path).ok();
            if cached_pipelines.is_some() {
                info!(path = ?cache_path, "found cached pipeline data");
            }

            let mut builder = ContextBuilder::new(adapter).auto_limits(&info);
            if let Some(ref data) = cached_pipelines {
                builder = builder.with_pipeline_cache(data.clone());
            }
            match builder.build().await {
                Ok(ctx) => {
                    info!("context created on: '{}'", ainfo.name);
                    context = Some(ctx);
                    gpu_coop = coop;
                    gpu_max_mb = max_mb;
                    gpu_info_name = ainfo.name;
                    gpu_device_type = ainfo.device_type;
                    break;
                }
                Err(e) => {
                    warn!("adapter '{}' failed: {}", ainfo.name, e);
                }
            }
        }

        let context = context.ok_or_else(|| {
            anyhow::anyhow!("no adapter could create a WebGPU context (tried {} adapters)", adapter_count)
        })?;
        info!(
            "selected GPU: '{}' (type={:?}, coop_matrix={}, max_buffer={}MB)",
            gpu_info_name, gpu_device_type, gpu_coop, gpu_max_mb
        );

        // --- Estimate model memory from actual shape ---
        // RWKV resident ≈ embed + head + KV state (weights stream through VRAM).
        // embed/head ≈ 2 * num_emb * vocab * 2 bytes (FP16).
        // state ≈ num_emb * num_layer * 4 * 4 bytes (4 state tensors per layer, FP32).
        let num_emb = info.num_emb as u64;
        let num_layer = info.num_layer as u64;
        let num_vocab = info.num_vocab as u64;
        let embed_head_fp16_mb = (2 * num_emb * num_vocab * 2) / (1024 * 1024);
        let state_fp32_mb = (num_emb * num_layer * 4 * 4) / (1024 * 1024);
        let resident_fp16_mb = embed_head_fp16_mb + state_fp32_mb;
        // Full model on-disk size ≈ params * 2 (FP16).
        let file_mb = std::fs::metadata(&model_path).map(|m| m.len() / (1024 * 1024)).unwrap_or(0);
        info!(
            "model memory: file={}MB  resident(FP16)={}MB  embed_head={}MB  state={}MB  layers={} emb={} vocab={}",
            file_mb, resident_fp16_mb, embed_head_fp16_mb, state_fp32_mb, num_layer, num_emb, num_vocab
        );

        // --- Quantization ---
        // Default: Int8 for all 32 layers (model is 5.5 GB FP16, doesn't fit in 4 GB VRAM).
        // Override with RWKV_QUANT env var: "none", "nf4=N", or "N" (Int8 N layers).
        let quant_spec_env = env::var("RWKV_QUANT").ok();
        let quant_layers: std::collections::HashMap<usize, Quant> = if let Some(ref qs) = quant_spec_env {
            if qs == "none" {
                info!("quantization: none (user override)");
                std::collections::HashMap::new()
            } else if let Some(n) = qs.strip_prefix("nf4=") {
                let n = n.parse::<usize>().unwrap_or(0);
                if n > 0 && !gpu_coop {
                    warn!("NF4 quantization requested but GPU lacks cooperative matrix support — may hang or fail");
                }
                let layers = (0..n).map(|l| (l, Quant::NF4)).collect();
                info!("quantization: NF4 {} layers (user override){}", n,
                    if !gpu_coop { " — GPU may not support this" } else { "" });
                layers
            } else if let Ok(n) = qs.parse::<usize>() {
                let layers = (0..n).map(|l| (l, Quant::Int8)).collect();
                info!("quantization: Int8 {} layers (user override)", n);
                layers
            } else {
                warn!("unknown RWKV_QUANT='{}', using default Int8 32", qs);
                default_quant()
            }
        } else {
            info!("quantization: Int8 32 layers (default)");
            default_quant()
        };

        if quant_layers.is_empty() {
            info!("quantization: none (weights stream through VRAM, resident only)");
        } else {
            let has_nf4 = quant_layers.values().any(|q| matches!(q, Quant::NF4));
            let has_int8 = quant_layers.values().any(|q| matches!(q, Quant::Int8));
            let label = match (has_nf4, has_int8) {
                (true, true) => "NF4+Int8",
                (true, false) => "NF4",
                (false, true) => "Int8",
                (false, false) => "unknown",
            };
            info!("quantization: {} layers ({})", quant_layers.len(), label);
        }

        // Set up quant cache directory for faster subsequent loads.
        // Pre-quantized Int8 weights will be saved here after first GPU quantize,
        // then loaded directly on future runs (skipping GPU quantize entirely).
        let quant_cache_dir = get_quant_cache_dir(&model_path);
        std::fs::create_dir_all(&quant_cache_dir).ok();
        let builder = ModelBuilder::new(&context, model)
            .quant(quant_layers)
            .quant_cache(quant_cache_dir);

        // --- Build the model (GPU weight upload) ---
        // ⚠ Debug builds: wgpu validation + unoptimized CPU code can cause GPU
        // driver timeouts inside build_v*(). If the process hangs here, rebuild
        // with `--release`. See the module docs for details.
        #[cfg(debug_assertions)]
        warn!(
            "Debug build detected! build_v7() may hang on some GPUs. \
             If this hangs, rebuild with `--release`."
        );

        let (runtime, state, initial_state) = match version {
            ModelVersion::V4 => {
                let m = builder.build_v4().await?;
                let b = web_rwkv::runtime::v4::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let init = s.init();
                let r = TokioRuntime::new(b).await;
                (r, AnyState::V4(Box::new(s)), init)
            }
            ModelVersion::V5 => {
                let m = builder.build_v5().await?;
                let b = web_rwkv::runtime::v5::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let init = s.init();
                let r = TokioRuntime::new(b).await;
                (r, AnyState::V5(Box::new(s)), init)
            }
            ModelVersion::V6 => {
                let m = builder.build_v6().await?;
                let b = web_rwkv::runtime::v6::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let init = s.init();
                let r = TokioRuntime::new(b).await;
                (r, AnyState::V6(Box::new(s)), init)
            }
            ModelVersion::V7 => {
                let m = builder.build_v7().await?;
                let b = v7::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let init = s.init();
                let r = TokioRuntime::new(b).await;
                (r, AnyState::V7(Box::new(s)), init)
            }
        };

        // Save pipeline cache for faster subsequent loads
        if let Some(data) = context.get_pipeline_cache_data() {
            let cache_path = get_pipeline_cache_path(&model_path);
            if let Some(parent) = cache_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            match std::fs::write(&cache_path, &data) {
                Ok(()) => info!(path = ?cache_path, size = data.len(), "saved pipeline cache"),
                Err(e) => warn!(path = ?cache_path, error = %e, "failed to save pipeline cache"),
            }
        }

        info!("RWKV runtime initialized");
        Ok(Self {
            context,
            runtime,
            state,
            initial_state,
            tokenizer,
            token_chunk_size,
            _model_data: model_data,
        })
    }

    async fn handle_complete(
        &mut self,
        system: &str,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
    ) -> Result<(String, TokenUsage), EngineError> {
        // Reset state to blank before each completion so independent requests
        // don't leak context into each other.
        self.state
            .load(self.initial_state.clone(), 0)
            .map_err(|e| EngineError::Backend(format!("state reset failed: {e}")))?;

        // RWKV-7 Chat models are trained on User:/Assistant: format.
        // System prompt goes first, then User:, then the model completes as Assistant:.
        let full = if system.is_empty() {
            format!("User: {prompt}\n\nAssistant:")
        } else {
            format!("{system}\n\nUser: {prompt}\n\nAssistant:")
        };
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
    ///
    /// **Blocks** until the model is fully loaded and the actor is ready to
    /// serve requests. Returns an error if model loading fails.
    pub fn from_env() -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<CompleteReq>(4);
        let (ready_tx, ready_rx) = oneshot::channel::<std::result::Result<(), String>>();

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
                            let _ = ready_tx.send(Ok(()));
                            actor.run(rx).await;
                        }
                        Err(e) => {
                            warn!("RWKV actor failed to initialize: {e}");
                            let _ = ready_tx.send(Err(format!("{e}")));
                        }
                    }
                });

                // Drive the local set until the actor thread is done.
                let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
                let _ = local.block_on(&rt, rx);
            })
            .expect("failed to spawn rwkv actor thread");

        // Block until the actor signals ready or fails.
        //
        // We use `futures::executor::block_on` here rather than tokio's
        // `Runtime::block_on` / `Handle::block_on` because this method may be
        // called from within a tokio `block_on` context (e.g. `#[tokio::main]`)
        // and both of those would panic with "Cannot start a runtime from within
        // a runtime".  `futures::executor::block_on` uses its own lightweight
        // executor that does not conflict with the outer tokio runtime.
        futures::executor::block_on(async {
            match ready_rx.await {
                Ok(Ok(())) => Ok::<_, anyhow::Error>(()),
                Ok(Err(msg)) => Err(anyhow::anyhow!("RWKV backend init failed: {msg}")),
                Err(_) => Err(anyhow::anyhow!("RWKV actor thread died before init")),
            }
        })?;

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
