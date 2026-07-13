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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

#[cfg(feature = "grammar-rwkv")]
use schoolmarm::{Grammar, GrammarState};

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

/// Resolve the GBNF grammar string for a [`CompletionRequest`].
///
/// Sources, in priority order:
///   1. `RWKV_GRAMMAR` environment variable (raw GBNF text).
///      Allows eval scripts to bind a grammar without wiring it through
///      `CompletionRequest::output_schema` (which the orchestrator already
///      fills with JSON schemas and would conflict with the BNF source).
///   2. Empty/missing \u2192 `None` \u2192 free-form generation.
#[cfg(feature = "grammar-rwkv")]
fn resolve_grammar(req: &CompletionRequest) -> Option<String> {
    if let Some(g) = req.grammar.as_ref() {
        if !g.trim().is_empty() {
            return Some(g.clone());
        }
    }
    match std::env::var("RWKV_GRAMMAR") {
        Ok(g) if !g.trim().is_empty() => Some(g),
        _ => None,
    }
}

/// Like `sample_token`, but restrict sampling to the token indices for which
/// `allowed[i]` is true. Disallowed logits are replaced with `NEG_INFINITY`
/// before the existing top-p/temperature walk.
///
/// `true` return value = a token was sampled. `false` = no allowed token
/// remained (the grammar has reached a state where nothing fits and the
/// caller should stop generating).
#[cfg_attr(not(feature = "grammar-rwkv"), allow(dead_code))]
fn constrained_sample_token(
    probs: &mut [f32],
    allowed: &[bool],
    temperature: f32,
    top_p: f32,
) -> Option<u32> {
    debug_assert_eq!(probs.len(), allowed.len(), "vocab length mismatch");
    let mut any_allowed = false;
    for (p, &ok) in probs.iter_mut().zip(allowed) {
        if !ok {
            *p = f32::NEG_INFINITY;
        } else {
            any_allowed = true;
        }
    }
    if !any_allowed {
        return None;
    }
    // Try top-p sampling on the full distribution (disallowed probs are
    // NEG_INFINITY so they sort to the bottom and will be dropped first
    // by the top-p walk). If top-p removes EVERYTHING (which can happen
    // when allowed tokens have negligible cumulative probability), fall
    // back to pure temperature sampling on the allowed subset only.
    let token = sample_token(probs, temperature, top_p);
    if token != 0 || allowed[0] {
        return Some(token);
    }
    // token 0 is special (EOS). If it's not in the allowed set, we must
    // pick something else. Gather the finite-probability (allowed) tokens
    // and temperature-sample from them.
    let candidates: Vec<(usize, f32)> = probs
        .iter()
        .enumerate()
        .filter(|(_, &p)| p.is_finite())
        .map(|(i, &p)| {
            let w = p.powf(1.0 / temperature);
            (i, w)
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let sum: f32 = candidates.iter().map(|(_, w)| w).sum();
    let r = fastrand::f32();
    let mut cum = 0.0f32;
    for (id, w) in &candidates {
        cum += w / sum;
        if r <= cum {
            return Some(*id as u32);
        }
    }
    candidates.last().map(|(id, _)| *id as u32)
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
    /// UTF-8 string for each token id, used by schoolmarm's `allowed_tokens`
    /// to compute per-step masks. Built once at actor construction; non-ASCII
    /// token bytes are mapped to the PUA range U+E000..U+E07F so that the
    /// BPE-style tokens schoolmarm expects survive the round-trip.
    /// `None` when the `grammar-rwkv` feature is disabled.
    #[cfg(feature = "grammar-rwkv")]
    token_strings: Vec<String>,
    token_chunk_size: usize,
    /// Keep model bytes alive as long as the actor exists.
    /// SafeTensors borrows from this, and ModelBuilder consumes SafeTensors
    /// before it returns, so this is just a safety net.
    #[allow(dead_code)]
    _model_data: Vec<u8>,
    /// Set to `true` to request cancellation of the current generation.
    cancel: Arc<AtomicBool>,
}

/// Request sent to the actor through the channel.
struct CompleteReq {
    system: String,
    prompt: String,
    max_tokens: usize,
    temperature: f32,
    /// Optional GBNF grammar; if set, every sampled token is masked by
    /// what the grammar accepts next. See `grammar-rwkv` feature.
    /// The gateway / non-feature-rwkv builds still receive it for
    /// future-proofing the message channel; the actor checks the inner
    /// value and only consumes it when the feature is on.
    #[cfg_attr(not(feature = "grammar-rwkv"), allow(dead_code))]
    grammar: Option<String>,
    reply: oneshot::Sender<Result<(String, TokenUsage), EngineError>>,
    /// When true, skip the state-reset step so the recurrent hidden
    /// state carries over from the previous call.
    preserve_state: bool,
    /// Called on the actor thread for every decoded token as it is generated.
    /// Passed through from [`CompletionRequest::on_token`].
    on_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
}

enum ActorMessage {
    Complete(CompleteReq),
    Cancel,
}

impl From<CompleteReq> for ActorMessage {
    fn from(req: CompleteReq) -> Self {
        Self::Complete(req)
    }
}

/// Get the path to the pipeline cache file for a given model.
///
/// Overridable via `RWKV_PIPELINE_CACHE_DIR=...`. When unset, falls back
/// to the historically-used `/tmp/roco-pipeline-cache`.
fn get_pipeline_cache_path(model_path: &str) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_path.hash(&mut hasher);
    let hash = hasher.finish();
    let root = std::env::var("RWKV_PIPELINE_CACHE_DIR")
        .unwrap_or_else(|_| "/tmp/roco-pipeline-cache".to_string());
    std::path::PathBuf::from(root).join(format!("{:016x}.bin", hash))
}

/// Get the directory for cached quantized Int8 weights.
///
/// Overridable via `RWKV_QUANT_CACHE_DIR=...`. When unset, falls back to
/// the historically-used `/tmp/roco-quant-cache`.
fn get_quant_cache_dir(model_path: &str) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_path.hash(&mut hasher);
    let hash = hasher.finish();
    let root = std::env::var("RWKV_QUANT_CACHE_DIR")
        .unwrap_or_else(|_| "/tmp/roco-quant-cache".to_string());
    std::path::PathBuf::from(root).join(format!("{:016x}", hash))
}

/// Auto-pick a quantization plan from the on-disk model size and GPU caps.
///
/// Replaces the old hardcoded `(0..32, Int8)` constant. The model declares
/// its own `num_layer` and `num_emb` via `Loader::info`; we don't pin a
/// layer count anywhere.
///
/// Policy:
/// * **None** if FP16 file < 1.5 GB on disk — fits on any GPU
///   (0.1B / 1.5B; ~ 200 MB / 1.34 GB).
/// * **NF4** if file ≥ 1.5 GB AND GPU has cooperative-matrix ops (NVIDIA
///   RTX 2050, AMD with coop support). NF4 is ~0.5× FP16 and faster matmul.
/// * **Int8** otherwise — universal safety net.
///
/// `RWKV_QUANT` env var still overrides: "none", "nf4=N", "N" (Int8 N layers).
///
/// `model_path` is used to read the on-disk file size (ground truth — wgpu's
/// `max_buffer_size` is unreliable: NVIDIA RTX 2050 reports 1 TB though it
/// actually has 4 GB).
fn auto_quant(
    info: &web_rwkv::runtime::model::ModelInfo,
    model_path: &str,
    gpu_coop: bool,
    _gpu_max_mb: u64,
) -> std::collections::HashMap<usize, Quant> {
    let num_layer = info.num_layer as u64;
    let num_emb = info.num_emb as u64;
    let num_vocab = info.num_vocab as u64;
    // Common RWKV convention: ffn_hidden = num_emb * 4 (or sometimes
    // num_emb * 3.5 for gated). We use *4 as a conservative upper bound.
    let ffn_hidden = num_emb * 4;

    // Approximate model param count:
    //   embed ≈ num_emb * num_vocab
    //   per layer: num_emb * num_emb (QKV-like) + 2 * num_emb * ffn_hidden (gated FFN)
    // For RWKV the Attention path uses some LoRA-like low-rank factors, so this
    // is a slightly-high estimate; binding to params means we'll lean toward
    // quantizing at the same weight thresholds the harness benchmarked.
    let params = (num_emb * num_vocab) + num_layer * (num_emb * num_emb + 2 * num_emb * ffn_hidden);
    let fp16_total_mb = (params * 2) / (1024 * 1024);

    let on_disk_mb = std::fs::metadata(model_path)
        .map(|m| m.len() / (1024 * 1024))
        .unwrap_or(fp16_total_mb);

    // Quantization policy for the rwkv7 family:
    //
    //   models < 1.5 GB on disk  → no quantization (model fits in any GPU)
    //   models ≥ 1.5 GB on disk  → quantize (most consumer GPUs have ≤ 4 GB VRAM
    //                               and wgpu's `max_buffer_size` is unreliable)
    //
    // We pick the quant type by GPU capability:
    //   NF4 cooperative-matrix available  → NF4 (faster matmul, ~0.5× size)
    //   otherwise                           → Int8 (universal safety net)
    //
    // `RWKV_QUANT` env var still overrides: "none", "nf4=N", "N" (Int8 N layers).
    let quantize_threshold_mb = 1536;

    if on_disk_mb < quantize_threshold_mb {
        info!(
            on_disk_mb = on_disk_mb,
            num_layer = num_layer,
            num_emb = num_emb,
            "small model (FP16 file {on_disk_mb}MB < {quantize_threshold_mb}MB) — no quantization"
        );
        return std::collections::HashMap::new();
    }

    let q = if gpu_coop { Quant::NF4 } else { Quant::Int8 };
    let label = if gpu_coop { "NF4" } else { "Int8" };
    let n_layers = info.num_layer;
    info!(
        on_disk_mb = on_disk_mb,
        gpu_coop = gpu_coop,
        num_layer = num_layer,
        "large model (FP16 file {on_disk_mb}MB >= {quantize_threshold_mb}MB) — \
         auto-quantizing all layers as {label} (wgpu VRAM reports are unreliable here)"
    );
    (0..n_layers).map(|l| (l, q)).collect()
}

/// Resolve the default model path when `RWKV_MODEL` env var is unset.
///
/// Strategy:
/// 1. If the user pinned a path with `RWKV_MODEL`, use it verbatim.
/// 2. Otherwise scan `<cwd>/models/` (or `<cwd>/../models/` via the symlink)
///    for any SafeTensors file matching `rwkv7-*` and pick the best one.
/// 3. Prefer the `-converted.st` variant (3D shapes ready for web-rwkv v7)
///    over raw `.st` (1D reshape needed).
/// 4. If no `.st` is found, error and list candidates the user could fetch.
///
/// This way the backend works **for any rwkv7 size** — 0.1B / 1.5B / 2.9B / 13B —
/// without code changes, as long as the matching SafeTensors file is present.
fn default_model_path() -> anyhow::Result<PathBuf> {
    let dir = std::env::current_dir().unwrap_or_default();

    // Try the project's local models dir, then the parent models dir (the
    // repo has a `models/` symlink). Skip if neither exists.
    let mut search_dirs: Vec<PathBuf> = Vec::new();
    for candidate in ["models", "../models"] {
        let p = dir.join(candidate);
        if p.is_dir() {
            search_dirs.push(p);
        }
    }
    if search_dirs.is_empty() {
        anyhow::bail!(
            "no models/ directory found (tried {dir:?}models and {dir:?}../models). \
             Set $RWKV_MODEL explicitly or place a rwkv7 .st file in models/."
        );
    }

    // Collect all rwkv7 .st files, scored:
    //   -converted.st         = 100 (the proven harness format, 3D shapes)
    //   -converted-*.st       =  90 (any converted variant)
    //   raw .st with rwkv7 in name = 50 (1D shapes, will mismatch web-rwkv)
    let mut best: Option<(i32, PathBuf)> = None;
    for search_dir in &search_dirs {
        let entries = match std::fs::read_dir(search_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in entries.flatten() {
            let path = e.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("rwkv7") || !name.ends_with(".st") {
                continue;
            }
            let score = if name.contains("-converted") {
                if name.contains("-converted.") || name == "model.st" {
                    100
                } else {
                    100
                }
            } else if name.ends_with("-converted.st") {
                100
            } else if name.contains("converted") {
                90
            } else if name.contains(".st") {
                50
            } else {
                0
            };
            if score == 0 {
                continue;
            }
            match &best {
                Some((s, _)) if *s >= score => {}
                _ => best = Some((score, path)),
            }
        }
    }

    match best {
        Some((_score, path)) => Ok(path),
        None => {
            // No ST file yet — give a helpful error listing what we saw.
            let mut listing = String::new();
            for search_dir in &search_dirs {
                if let Ok(entries) = std::fs::read_dir(search_dir) {
                    for e in entries.flatten() {
                        if let Some(_name) = e.path().file_name().and_then(|n| n.to_str()) {
                            listing.push_str(&format!(
                                "  {} ({})\n",
                                e.path().display(),
                                std::fs::metadata(e.path())
                                    .map(|m| format!("{}MB", m.len() / (1024 * 1024)))
                                    .unwrap_or_default()
                            ));
                        }
                    }
                }
            }
            anyhow::bail!(
                "no rwkv7 .st file found in any of {:?}.\n\
                 Models on disk:\n{listing}\n\
                 Hint: convert a GGUF to SafeTensors first (scripts/convert_gguf_to_st.py), \
                 or set $RWKV_MODEL explicitly.",
                search_dirs
            )
        }
    }
}

impl RwkvActor {
    async fn from_env() -> anyhow::Result<Self> {
        // Resolve model path: explicit override (RWKV_MODEL) wins; otherwise
        // auto-pick the best rwkv7 *.st file present on disk. See default_model_path.
        let model_path: PathBuf = match env::var("RWKV_MODEL") {
            Ok(p) => PathBuf::from(p),
            Err(_) => default_model_path()?,
        };
        // The rest of the pipeline expects `String` (interpolation,
        // tokio::fs::read_to_string, etc.). Convert once.
        let model_path = model_path.to_string_lossy().to_string();
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
        let token_chunk_size: usize = env::var("RWKV_CHUNK")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(128);

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
                let coop = a
                    .features()
                    .contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);
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
                    i.name,
                    i.device_type,
                    coop,
                    max_buf_mb,
                    i.backend
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
            let coop = adapter
                .features()
                .contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);
            let max_mb = adapter.limits().max_buffer_size / (1024 * 1024);

            // If user requested a specific adapter, skip others.
            if let Some(ref filter) = adapter_name_filter {
                if !ainfo.name.to_lowercase().contains(&filter.to_lowercase()) {
                    continue;
                }
            }

            info!(
                "trying adapter: '{}' (type={:?}, coop={}, {}MB)",
                ainfo.name, ainfo.device_type, coop, max_mb
            );

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
            anyhow::anyhow!(
                "no adapter could create a WebGPU context (tried {} adapters)",
                adapter_count
            )
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
        let file_mb = std::fs::metadata(&model_path)
            .map(|m| m.len() / (1024 * 1024))
            .unwrap_or(0);
        info!(
            "model memory: file={}MB  resident(FP16)={}MB  embed_head={}MB  state={}MB  layers={} emb={} vocab={}",
            file_mb, resident_fp16_mb, embed_head_fp16_mb, state_fp32_mb, num_layer, num_emb, num_vocab
        );

        // --- Quantization ---
        // Override with RWKV_QUANT env var: "none", "nf4=N", or "N" (Int8 N layers).
        // If unset, auto-pick from model shape + GPU capabilities (see auto_quant).
        let quant_spec_env = env::var("RWKV_QUANT").ok();
        let quant_layers: std::collections::HashMap<usize, Quant> = if let Some(ref qs) =
            quant_spec_env
        {
            if qs == "none" {
                info!("quantization: none (user override)");
                std::collections::HashMap::new()
            } else if let Some(n) = qs.strip_prefix("nf4=") {
                let n = n.parse::<usize>().unwrap_or(0);
                let n = n.min(info.num_layer);
                if n > 0 && !gpu_coop {
                    warn!("NF4 quantization requested but GPU lacks cooperative matrix support — may hang or fail");
                }
                let layers = (0..n).map(|l| (l, Quant::NF4)).collect();
                info!(
                    "quantization: NF4 {n} of {} layers (user override){}",
                    info.num_layer,
                    if !gpu_coop {
                        " — GPU may not support this"
                    } else {
                        ""
                    }
                );
                layers
            } else if let Ok(n) = qs.parse::<usize>() {
                let n = n.min(info.num_layer);
                let layers = (0..n).map(|l| (l, Quant::Int8)).collect();
                info!(
                    "quantization: Int8 {n} of {info_num} layers (user override)",
                    info_num = info.num_layer
                );
                layers
            } else {
                warn!("unknown RWKV_QUANT='{qs}', falling back to auto-quant from model shape");
                auto_quant(&info, &model_path, gpu_coop, gpu_max_mb)
            }
        } else {
            auto_quant(&info, &model_path, gpu_coop, gpu_max_mb)
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
        #[cfg(feature = "grammar-rwkv")]
        let token_strings = {
            let bytes = tokenizer.token_index_to_bytes();
            bytes
                .iter()
                .map(|b| {
                    let mut s = String::with_capacity(b.len());
                    for &byte in b {
                        if byte < 0x80 {
                            s.push(byte as char);
                        } else {
                            s.push(char::from_u32(0xE000 + (byte as u32 - 0x80)).unwrap());
                        }
                    }
                    s
                })
                .collect::<Vec<String>>()
        };
        Ok(Self {
            context,
            runtime,
            state,
            initial_state,
            tokenizer,
            #[cfg(feature = "grammar-rwkv")]
            token_strings,
            token_chunk_size,
            _model_data: model_data,
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }

    async fn handle_complete(
        &mut self,
        system: &str,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        preserve_state: bool,
        on_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
        #[cfg(feature = "grammar-rwkv")] grammar: Option<&str>,
    ) -> Result<(String, TokenUsage), EngineError> {
        // Grammar setup: compile a fresh GrammarState per call. The state is
        // single-thread by API design (schoolmarm carries no `Sync`) and only
        // meaningful for the lifetime of this completion, so we own it here.
        #[cfg(feature = "grammar-rwkv")]
        let mut grammar_state: Option<GrammarState> =
            match grammar {
                Some(g) if !g.trim().is_empty() => {
                    let compiled = Grammar::new(g)
                        .map_err(|e| EngineError::Backend(format!("GBNF compile error: {e:?}")))?;
                    Some(GrammarState::new(compiled).map_err(|e| {
                        EngineError::Backend(format!("GrammarState init error: {e:?}"))
                    })?)
                }
                _ => None,
            };
        #[cfg(feature = "grammar-rwkv")]
        let vocab_refs: Option<Vec<&str>> = grammar_state
            .as_ref()
            .map(|_| self.token_strings.iter().map(String::as_str).collect());

        // Reset state to blank before each completion so independent requests
        // don't leak context into each other (unless `preserve_state` is set).
        if !preserve_state {
            self.state
                .load(self.initial_state.clone(), 0)
                .map_err(|e| EngineError::Backend(format!("state reset failed: {e}")))?;
        }

        // RWKV-7 Chat models are trained on role-prefixed dialogue
        // (System:/User:/Assistant:). Keep the full conversation history
        // with explicit role markers.
        let full = if system.is_empty() {
            format!("User: {prompt}\n\nAssistant:")
        } else {
            format!("System: {system}\n\nUser: {prompt}\n\nAssistant:")
        };
        let prompt_tokens = self
            .tokenizer
            .encode(full.as_bytes())
            .map_err(|e| EngineError::Backend(format!("tokenizer encode: {e}")))?;
        let prompt_len = prompt_tokens.len();

        let top_p = if temperature < 0.3 {
            0.8
        } else if temperature < 0.7 {
            0.9
        } else {
            0.95
        };

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
            if self.cancel.load(Ordering::Relaxed) {
                return Ok((
                    text,
                    TokenUsage {
                        prompt_tokens: prompt_len,
                        completion_tokens: generated.len(),
                    },
                ));
            }
            let input = inference.clone();
            let (input, output) = self.runtime.infer(input).await.map_err(|e| {
                EngineError::Backend(format!("RWKV inference (first token): {e:?}"))
            })?;
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

            // Sample, optionally constrained by a GBNF grammar.
            #[cfg(feature = "grammar-rwkv")]
            let (token, grammar_active) = {
                let mut p = probs.data().to_vec();
                let token: u32;
                let active: bool;
                if let (Some(gs), Some(vrefs)) = (grammar_state.as_mut(), vocab_refs.as_ref()) {
                    let allowed = gs.allowed_tokens(vrefs);
                    match constrained_sample_token(&mut p, &allowed, temperature, top_p) {
                        Some(t) => {
                            token = t;
                            active = true;
                        }
                        None => break, // no allowed token → stop generation
                    }
                } else {
                    token = sample_token(&p, temperature, top_p);
                    active = false;
                }
                (token, active)
            };
            #[cfg(not(feature = "grammar-rwkv"))]
            let token = sample_token(probs.data(), temperature, top_p);

            if token == 0 {
                break;
            }

            let decoded = self
                .tokenizer
                .decode(&[token])
                .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
            let word = String::from_utf8_lossy(&decoded).to_string();

            // Notify the streaming callback if one was provided.
            if let Some(ref cb) = on_token {
                cb(&word);
            }

            // Advance grammar state with the bytes of the chosen token.
            // Tolerate failures: some BPE chunkings straddle a literal
            // boundary; a clean termination is "grammar finished — input
            // has nothing meaningful to add", not an error.
            #[cfg(feature = "grammar-rwkv")]
            if grammar_active {
                if let Some(gs) = grammar_state.as_mut() {
                    let _ = gs.accept_token(&word);
                }
            }

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
            return Ok((
                text,
                TokenUsage {
                    prompt_tokens: prompt_len,
                    completion_tokens: 0,
                },
            ));
        }

        // Generate remaining tokens
        for _ in 1..max_tokens {
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            let input = inference.clone();
            let (input, output) =
                self.runtime.infer(input).await.map_err(|e| {
                    EngineError::Backend(format!("RWKV inference (gen step): {e:?}"))
                })?;
            inference = input;

            let ot = output[0].0.clone();
            if ot.size() == 0 {
                break;
            }

            let data = ot.to_vec();
            let shape = ot.shape();
            let cpu = TensorCpu::from_data(shape, data)
                .map_err(|e| EngineError::Backend(format!("tensor creation: {e}")))?;
            let probs = softmax_one(&self.context, cpu)
                .await
                .map_err(|e| EngineError::Backend(format!("softmax: {e}")))?;
            // Sample, optionally constrained by a GBNF grammar.
            #[cfg(feature = "grammar-rwkv")]
            let token_opt: Option<u32> = {
                let mut p = probs.data().to_vec();
                if let (Some(gs), Some(vrefs)) = (grammar_state.as_mut(), vocab_refs.as_ref()) {
                    let allowed = gs.allowed_tokens(vrefs);
                    constrained_sample_token(&mut p, &allowed, temperature, top_p)
                } else {
                    Some(sample_token(&p, temperature, top_p))
                }
            };
            #[cfg(not(feature = "grammar-rwkv"))]
            let token_opt: Option<u32> = Some(sample_token(probs.data(), temperature, top_p));

            let token = match token_opt {
                Some(t) => t,
                None => break,
            };

            if token == 0 {
                break;
            }

            let decoded = self
                .tokenizer
                .decode(&[token])
                .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
            let word = String::from_utf8_lossy(&decoded).to_string();

            // Notify the streaming callback if one was provided.
            if let Some(ref cb) = on_token {
                cb(&word);
            }

            // Advance grammar state with the bytes of the sampled token.
            #[cfg(feature = "grammar-rwkv")]
            if let Some(gs) = grammar_state.as_mut() {
                let _ = gs.accept_token(&word);
            }

            if word == "\n\n" || word == "\nUser:" || word == "\nHuman:" {
                break;
            }

            text.push_str(&word);
            generated.push(token);
            inference.batches[0] = RnnInputBatch::new(vec![token], RnnOption::Last);
        }

        let text = if generated.is_empty() {
            return Err(EngineError::EmptyResponse);
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
    async fn run(mut self, mut rx: mpsc::Receiver<ActorMessage>) {
        use ActorMessage::*;
        while let Some(msg) = rx.recv().await {
            match msg {
                Complete(req) => {
                    self.cancel.store(false, Ordering::Relaxed);
                    let result = self
                        .handle_complete(
                            &req.system,
                            &req.prompt,
                            req.max_tokens,
                            req.temperature,
                            req.preserve_state,
                            req.on_token,
                            #[cfg(feature = "grammar-rwkv")]
                            req.grammar.as_deref(),
                        )
                        .await;
                    let _ = req.reply.send(result);
                }
                Cancel => {
                    self.cancel.store(true, Ordering::Relaxed);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Backend — thread-safe handle to the actor thread
// ---------------------------------------------------------------------------

pub struct RwkvBackend {
    tx: Option<mpsc::Sender<ActorMessage>>,
    /// Join handle of the dedicated actor thread. Joined on drop so the
    /// thread's wgpu/tokio resources are torn down in-order (see `Drop`).
    actor_thread: Option<std::thread::JoinHandle<()>>,
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
        let (tx, rx) = mpsc::channel::<ActorMessage>(4);
        let (ready_tx, ready_rx) = oneshot::channel::<std::result::Result<(), String>>();

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
                            warn!("RWKV actor failed to initialize: {e}");
                            let _ = ready_tx.send(Err(format!("{e}")));
                        }
                    }
                });

                // Drive the local set until the actor task finishes. The task
                // ends when the caller drops `RwkvBackend` (closing the request
                // channel). Awaiting the join handle instead of a never-sent
                // oneshot lets the thread exit cleanly, so its wgpu/tokio
                // resources drop in-order on this thread — previously the
                // thread leaked (blocked on an unsent oneshot) and was killed
                // at process exit, corrupting the allocator (`free(): invalid size`).
                let _ = local.block_on(&rt, actor_handle);
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
            tx: Some(tx),
            actor_thread: Some(actor_thread),
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
        let tx = self
            .tx
            .clone()
            .expect("rwkv backend already shut down (channel closed)");
        Box::pin(async move {
            let started = Instant::now();
            let (reply_tx, reply_rx) = oneshot::channel();
            // Resolve the grammar *before* moving the request fields into
            // `CompleteReq` so we don't partially-borrow by accident.
            //
            // The grammar field is unconditional on `CompleteReq`; we
            // populate `None` when the `grammar-rwkv` feature is off so
            // the actor's `handle_complete` signature stays uniform.
            #[cfg(feature = "grammar-rwkv")]
            let grammar = resolve_grammar(&req);
            #[cfg(not(feature = "grammar-rwkv"))]
            let grammar: Option<String> = None;
            tx.send(
                CompleteReq {
                    system: req.system,
                    prompt: req.prompt,
                    max_tokens: req.max_tokens,
                    temperature: req.temperature,
                    grammar,
                    reply: reply_tx,
                    preserve_state: req.preserve_state,
                    on_token: req.on_token,
                }
                .into(),
            )
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
                think_trace: None,
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
}

impl Drop for RwkvBackend {
    fn drop(&mut self) {
        // Close the request channel first so the actor loop's `rx.recv()`
        // returns `None` and the actor task ends. Then join the actor thread,
        // which lets its wgpu/tokio resources drop in-order on the thread that
        // owns them. Without this, the actor thread was never joined: it
        // blocked on a never-sent oneshot, was killed at process exit, and its
        // still-live wgpu/tokio allocator state was torn down by the OS —
        // producing `free(): invalid size` at shutdown.
        self.tx.take();
        if let Some(handle) = self.actor_thread.take() {
            let _ = handle.join();
        }
    }
}
