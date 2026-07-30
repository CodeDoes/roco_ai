//! The RWKV actor thread — owns all non-Send GPU resources.
//!
//! [`RwkvActor`] runs on a dedicated OS thread with a single-threaded tokio
//! runtime, communicating with the outside world through channels. This
//! works around `web-rwkv`'s async methods producing non-`Send` futures.

use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use half::f16;
use rand::rngs::StdRng;
use rand::SeedableRng;
use roco_engine::{BnfMask, EngineError, TokenUsage};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use web_rwkv::context::{Context, ContextBuilder};
use web_rwkv::runtime::infer::{Rnn, RnnInput, RnnInputBatch, RnnOption};
use web_rwkv::runtime::loader::{Loader, Reader as _};
use web_rwkv::runtime::model::State as RwkvState;
use web_rwkv::runtime::model::{Bundle, ContextAutoLimits, ModelBuilder, ModelVersion, Quant};
use web_rwkv::runtime::softmax::softmax_one;
use web_rwkv::runtime::v7;
use web_rwkv::runtime::TokioRuntime;
use web_rwkv::tensor::{TensorCpu, TensorError, TensorInit, TensorShape};
use web_rwkv::tokenizer::Tokenizer;

// NOTE: roco-bnf-engine MUST NOT be imported here — its kbnf types
// trigger a compiler overflow (string-interner recursion) when they
// appear in the same compilation unit as web-rwkv's TokioRuntime.
// Grammar constraints are pre-built as Box<dyn BnfMask> outside this crate.

use crate::config::{
    auto_quant, check_model_cache, default_model_path, get_pipeline_cache_path,
    get_quant_cache_dir, migrate_pipeline_cache, CacheIndex, CachedTensorInfo,
};
use crate::sampling;

// ---------------------------------------------------------------------------
// CachedReader — serves tensor data from disk cache, bypassing .st file
// ---------------------------------------------------------------------------

/// A `Reader` implementation that loads tensor data from pre-cached files
/// on disk. Used on subsequent loads after the first successful model build
/// has populated the quant cache + vector cache.
///
/// The `.st` file is NOT read at all when cache is complete — only GPU
/// is used for model building (and for quantized matrices, the quant cache).
pub(crate) struct CachedReader {
    /// All tensor names from the cache index
    names: Vec<String>,
    /// Quick-lookup: name -> (dtype_str, shape)
    metadata: HashMap<String, (String, Vec<usize>)>,
    /// Cache directory root
    cache_dir: PathBuf,
}

impl CachedReader {
    /// Create from a complete cache index + on-disk files (no .st file needed).
    pub fn from_cache(cache_dir: PathBuf) -> anyhow::Result<Self> {
        let index = CacheIndex::load(&cache_dir)
            .ok_or_else(|| anyhow::anyhow!("cache index not found in {:?}", cache_dir))?;
        let mut names = Vec::new();
        let mut metadata = HashMap::new();
        for t in &index.tensors {
            names.push(t.name.clone());
            metadata.insert(t.name.clone(), (t.dtype.clone(), t.shape.clone()));
        }
        info!(
            "CachedReader: {} tensors from cache index in {:?}",
            names.len(),
            cache_dir
        );
        Ok(Self {
            names,
            metadata,
            cache_dir,
        })
    }
}

impl web_rwkv::runtime::loader::Reader for CachedReader {
    fn names(&self) -> Vec<&str> {
        self.names.iter().map(|s| s.as_str()).collect()
    }

    fn contains(&self, name: &str) -> bool {
        self.metadata.contains_key(name)
    }

    fn shape(&self, name: &str) -> Result<Vec<usize>, safetensors::SafeTensorError> {
        self.metadata
            .get(name)
            .map(|(_, shape)| shape.clone())
            .ok_or_else(|| {
                tracing::warn!("CachedReader::shape: tensor not found: {name}");
                safetensors::SafeTensorError::TensorNotFound(name.to_string())
            })
    }

    fn tensor(
        &self,
        name: &str,
    ) -> Result<
        (safetensors::Dtype, Vec<usize>, std::borrow::Cow<'_, [u8]>),
        safetensors::SafeTensorError,
    > {
        // Load from cache
        let safe = name.replace(['.', '/', '\\', ':'], "_");
        let vec_path = self.cache_dir.join(format!("{safe}_vec.bin"));
        let data = std::fs::read(&vec_path).map_err(|e| {
            safetensors::SafeTensorError::TensorNotFound(format!(
                "{name}: {safe}_vec.bin not found in {:?}: {e}",
                self.cache_dir
            ))
        })?;

        let (dtype_str, shape) = self
            .metadata
            .get(name)
            .ok_or_else(|| safetensors::SafeTensorError::TensorNotFound(name.to_string()))?;

        let dtype = match dtype_str.as_str() {
            "F32" => safetensors::Dtype::F32,
            "F16" => safetensors::Dtype::F16,
            "BF16" => safetensors::Dtype::BF16,
            _ => {
                return Err(safetensors::SafeTensorError::TensorNotFound(format!(
                    "{name}: unsupported dtype {dtype_str}"
                )))
            }
        };

        Ok((dtype, shape.clone(), std::borrow::Cow::Owned(data)))
    }
}

// ---------------------------------------------------------------------------
// MmapReader — wraps a memory-mapped .st file as a web-rwkv Reader.
// Used on first-time (cache-miss) loads. The mmap'd data is paged by the
// OS; only pages actually accessed (layer-by-layer during build_v7) get
// loaded into RAM.
// ---------------------------------------------------------------------------

/// A `Reader` that wraps a memory-mapped `.st` file. Used on first-time
/// loads when no cache exists. The mmap stays valid for the duration of
/// the `ModelBuilder.build_v7()` call, which loads tensors layer-by-layer.
/// `_mmap` is held alive only to keep the memory mapping valid.
struct MmapReader {
    /// SAFETY: `st` must be dropped BEFORE `_mmap`. Rust drops struct fields
    /// in declaration order, so `st` (declared first) is dropped first.
    st: safetensors::SafeTensors<'static>,
    _mmap: memmap2::Mmap,
}

impl MmapReader {
    fn new(mmap: memmap2::Mmap) -> anyhow::Result<Self> {
        // SafeTensors borrows the mmap data. We extend the lifetime to
        // 'static because the mmap is owned by this struct and lives as
        // long as the SafeTensors. Since `st` is declared before `_mmap`,
        // Rust drops `st` first (dropping the borrow), then drops `_mmap`.
        let data: &[u8] = &mmap;
        let st = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(data) };
        let st = safetensors::SafeTensors::deserialize(st)?;
        Ok(Self { st, _mmap: mmap })
    }
}

impl web_rwkv::runtime::loader::Reader for MmapReader {
    fn names(&self) -> Vec<&str> {
        self.st.names()
    }

    fn contains(&self, name: &str) -> bool {
        self.st.contains(name)
    }

    fn shape(&self, name: &str) -> Result<Vec<usize>, safetensors::SafeTensorError> {
        Ok(self.st.tensor(name)?.shape().to_vec())
    }

    fn tensor(
        &self,
        name: &str,
    ) -> Result<
        (safetensors::Dtype, Vec<usize>, std::borrow::Cow<'_, [u8]>),
        safetensors::SafeTensorError,
    > {
        let view = self.st.tensor(name)?;
        Ok((
            view.dtype(),
            view.shape().to_vec(),
            std::borrow::Cow::Borrowed(view.data()),
        ))
    }
}

impl MmapReader {
    /// Before `build_v7()` runs, save ALL tensor data to the cache directory
    /// as `_vec.bin` files. The mmap pages each tensor in one at a time as
    /// we write it; the OS can evict pages after each write. This avoids ever
    /// having the full 5.6GB in RAM at once.
    ///
    /// During `build_v7()`, web-rwkv's quant cache saves `_q.bin` / `_m.bin`
    /// for quantized matrices on top of these `_vec.bin` files. The post-build
    /// step removes `_vec.bin` where `_q.bin` exists.
    fn cache_small_tensors(&self, cache_dir: &std::path::Path) -> anyhow::Result<()> {
        use std::io::Write;
        std::fs::create_dir_all(cache_dir)?;

        for name in self.names() {
            // Read tensor data through the Reader interface (from mmap).
            // The mmap only pages in the bytes we actually access.
            let (dtype, _shape, data) = match web_rwkv::runtime::loader::Reader::tensor(self, name)
            {
                Ok(t) => t,
                Err(_) => continue,
            };
            match dtype {
                safetensors::Dtype::F32 | safetensors::Dtype::F16 | safetensors::Dtype::BF16 => {}
                _ => continue,
            }

            let safe = name.replace(['.', '/', '\\', ':'], "_");
            let vec_path = cache_dir.join(format!("{safe}_vec.bin"));
            if vec_path.exists() {
                continue; // already cached
            }

            // Write to disk. The mmap pages in this tensor's data, we write
            // it, then the page can be evicted by the OS.
            let mut f = std::fs::File::create(&vec_path)?;
            f.write_all(data.as_ref())?;
        }

        Ok(())
    }
}

/// Unifies `CachedReader` and `MmapReader` into a single type for
/// `ModelBuilder::new()`, which takes a concrete generic `R: Reader`.
enum ReaderBox {
    Cached(CachedReader),
    Mmap(MmapReader),
}

impl web_rwkv::runtime::loader::Reader for ReaderBox {
    fn names(&self) -> Vec<&str> {
        match self {
            ReaderBox::Cached(r) => r.names(),
            ReaderBox::Mmap(r) => r.names(),
        }
    }

    fn contains(&self, name: &str) -> bool {
        match self {
            ReaderBox::Cached(r) => r.contains(name),
            ReaderBox::Mmap(r) => r.contains(name),
        }
    }

    fn shape(&self, name: &str) -> Result<Vec<usize>, safetensors::SafeTensorError> {
        match self {
            ReaderBox::Cached(r) => r.shape(name),
            ReaderBox::Mmap(r) => r.shape(name),
        }
    }

    fn tensor(
        &self,
        name: &str,
    ) -> Result<
        (safetensors::Dtype, Vec<usize>, std::borrow::Cow<'_, [u8]>),
        safetensors::SafeTensorError,
    > {
        match self {
            ReaderBox::Cached(r) => r.tensor(name),
            ReaderBox::Mmap(r) => r.tensor(name),
        }
    }
}

// ---------------------------------------------------------------------------
// Type-erased state
// ---------------------------------------------------------------------------

pub(crate) enum AnyState {
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
// Request / message types
// ---------------------------------------------------------------------------

pub struct CompleteReq {
    /// Raw text — inferd does NOT add System/User/Assistant formatting.
    pub prompt: String,
    pub prefill: Option<String>,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_a: Option<f32>,
    pub grammar: Option<String>,
    /// Opaque grammar constraint callback, created outside this crate
    /// so grammar-engine types never enter this compilation unit.
    pub bnf_mask: Option<Box<dyn BnfMask>>,
    pub reply:
        oneshot::Sender<Result<(String, TokenUsage, Vec<roco_engine::TokenTrace>), EngineError>>,
    pub on_token: roco_engine::OnToken,
    /// Load a named state from the cache before processing.
    pub state_id: Option<String>,
    /// Save the resulting state under this name after processing.
    pub save_as: Option<String>,
    /// Wall-clock deadline for the entire completion in milliseconds.
    /// 0 = no deadline.
    pub deadline_ms: u64,
    /// Deterministic seed for reproducible sampling.
    pub seed: Option<u64>,
    /// Record per-token sampling metadata for trace logging.
    pub record_trace: bool,
}

pub struct BlendReq {
    pub session_a: String,
    pub session_b: String,
    pub alpha: f32,
    pub output_session: String,
    pub reply: oneshot::Sender<Result<(), EngineError>>,
}

pub enum ActorMessage {
    Complete(CompleteReq),
    BlendStates(BlendReq),
    Cancel,
    #[cfg(feature = "grammar")]
    GetVocabBytes(oneshot::Sender<Vec<Vec<u8>>>),
    /// Serialize the current recurrent state to bytes.
    SaveState(oneshot::Sender<Result<Vec<u8>, EngineError>>),
    /// Restore a recurrent state previously produced by `SaveState`.
    LoadState(Vec<u8>, oneshot::Sender<Result<(), EngineError>>),
    /// Baked state: feed text through model, save resulting state.
    /// Loads `state_id` first if provided, then processes `text`,
    /// then saves result as `name`.
    Bake {
        state_id: Option<String>,
        text: String,
        name: String,
        reply: oneshot::Sender<Result<(), EngineError>>,
    },
    /// Feed token 0 (EOS) to update recurrent state without generating.
    FeedEos(Option<String>),
}

impl From<CompleteReq> for ActorMessage {
    fn from(req: CompleteReq) -> Self {
        Self::Complete(req)
    }
}

impl From<BlendReq> for ActorMessage {
    fn from(req: BlendReq) -> Self {
        Self::BlendStates(req)
    }
}

// Session name used for FIM (fill-in-the-middle) state-tuning.
// When this session is active, the model has few-shot FIM examples baked
// into its recurrent state. After generating the INSERT completion, we
// must force-terminate with token 0 to prevent the model from continuing
// the few-shot dialogue pattern.
pub const FIM_SESSION_NAME: &str = "roco_fim";

/// Stop-pattern strings encoded at init time from the loaded vocab.
/// These guard against the model echoing FIM scaffolding or continuing
/// a multi-turn dialogue in the prompt template.
pub const STOP_PATTERNS: &[&str] = &[
    "\n\n",
    "User:",
    "Human:",
    "NOW",
    "BEFORE:",
    "AFTER:",
    "INSERT:",
    "User: NOW",
];

// ---------------------------------------------------------------------------
// Actor
// ---------------------------------------------------------------------------

pub struct RwkvActor {
    pub context: Context,
    pub runtime: TokioRuntime<Rnn>,
    pub(crate) state: AnyState,
    pub initial_state: TensorCpu<f32>,
    pub tokenizer: Tokenizer,
    /// Vocab bytes (token_id → raw bytes) used by application layer to create
    /// `BnfMask` instances. Stored as plain bytes — no kbnf types ever enter
    /// this crate.
    pub vocab_bytes: Vec<Vec<u8>>,
    pub token_chunk_size: usize,
    // model_data intentionally NOT stored — freed after build_v7()
    // to avoid pinning ~5GB of system RAM for the raw .st file.
    pub cancel: Arc<AtomicBool>,
    pub state_pool: HashMap<String, Option<TensorCpu<f32>>>,
    pub session_lru: VecDeque<String>,
    pub max_sessions: usize,
    /// Token-ID sequences for stop-pattern matching, encoded from the vocab
    /// at init time so they reflect the actual tokenizer vocabulary.
    pub stop_token_sequences: Vec<Vec<u32>>,
}

impl RwkvActor {
    pub async fn from_env() -> anyhow::Result<Self> {
        let model_path: PathBuf = match env::var("RWKV_MODEL") {
            Ok(p) => PathBuf::from(p),
            Err(_) => default_model_path()?,
        };
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
        let vocab_bytes = tokenizer.token_index_to_bytes().to_vec();

        // Track .st file size for reporting (only loaded from metadata, not data)
        let mut file_len: Option<u64> = None;

        // -------------------------------------------------------------------
        // Step 1: Check for complete model cache on disk
        // -------------------------------------------------------------------
        let quant_cache_dir = get_quant_cache_dir(&model_path);
        let is_cache_hit = check_model_cache(&model_path).is_some();

        let (model_reader, model_info) = if is_cache_hit {
            // Cache is complete — build a CachedReader, no .st file needed.
            info!(
                "complete model cache found in {:?} — skipping .st file (GPU only)",
                quant_cache_dir
            );
            let reader = CachedReader::from_cache(quant_cache_dir.clone())?;
            let info = Loader::info(&reader)?;
            (ReaderBox::Cached(reader), info)
        } else {
            // First-time load — memory-map .st; do NOT iterate all tensors.
            // build_v7() loads layer-by-layer from mmap, and web-rwkv's
            // quant cache populates during load_matrix(). Only pages that
            // are actually accessed get paged in by the OS.
            info!(
                "no cache found — memory-mapping .st file: {} (layer-by-layer)",
                model_path
            );
            let file = std::fs::File::open(&model_path)
                .map_err(|e| anyhow::anyhow!("failed to open .st file {}: {e}", model_path))?;
            let mmap = unsafe { memmap2::Mmap::map(&file) }
                .map_err(|e| anyhow::anyhow!("failed to mmap .st file {}: {e}", model_path))?;
            file_len = Some(
                file.metadata()
                    .map(|m| m.len() / (1024 * 1024))
                    .unwrap_or(0),
            );

            let mmap_reader = MmapReader::new(mmap)?;
            let info = Loader::info(&mmap_reader)?;

            // Before build_v7, cache only small tensors (vectors, layernorms,
            // biases, tiny matrices). This is ~150MB total — safe to copy.
            // Large weight matrices will be cached by web-rwkv's quant cache
            // during build_v7(). This avoids iterating the full 5.6GB file.
            info!(
                "caching small tensors (vectors, biases, LN) in {:?}…",
                quant_cache_dir
            );
            mmap_reader.cache_small_tensors(&quant_cache_dir)?;
            (ReaderBox::Mmap(mmap_reader), info)
        };

        // Save tensor metadata (names + shapes) before the reader is
        // consumed by ModelBuilder. Used after build_v7 to build the cache index.
        let tensor_metadata: HashMap<String, Vec<usize>> = model_reader
            .names()
            .iter()
            .filter_map(|name| {
                model_reader
                    .shape(name)
                    .ok()
                    .map(|shape| (name.to_string(), shape))
            })
            .collect();
        info!("extracted metadata for {} tensors", tensor_metadata.len());

        let version = model_info.version;
        info!(version = ?version, layers = model_info.num_layer, vocab = model_info.num_vocab, emb = model_info.num_emb, "model info");

        // -------------------------------------------------------------------
        // GPU enumeration (uses model_info for buffer size estimation)
        // -------------------------------------------------------------------
        let instance = wgpu::Instance::default();
        let all_adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
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

        let adapter_name_filter = env::var("RWKV_ADAPTER").ok();
        let adapter_count = scored.len();

        let mut context: Option<Context> = None;
        let mut gpu_coop = false;
        let mut gpu_max_mb = 0u64;
        let mut gpu_info_name = String::new();

        for (adapter, _score) in scored {
            let ainfo = adapter.get_info();
            let coop = adapter
                .features()
                .contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);
            let max_mb = adapter.limits().max_buffer_size / (1024 * 1024);
            if let Some(ref filter) = adapter_name_filter {
                if !ainfo.name.to_lowercase().contains(&filter.to_lowercase()) {
                    continue;
                }
            }
            info!(
                "trying adapter: '{}' (type={:?}, coop={}, {}MB)",
                ainfo.name, ainfo.device_type, coop, max_mb
            );

            // Attempt migration from old path-based pipeline cache
            migrate_pipeline_cache(&model_path);
            let cache_path = get_pipeline_cache_path(&model_path);
            let cached_pipelines = std::fs::read(&cache_path).ok();
            let mut builder = ContextBuilder::new(adapter).auto_limits(&model_info);
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
                    break;
                }
                Err(e) => warn!("adapter '{}' failed: {}", ainfo.name, e),
            }
        }

        let context = context.ok_or_else(|| {
            anyhow::anyhow!(
                "no adapter could create a WebGPU context (tried {} adapters)",
                adapter_count
            )
        })?;
        info!(
            "selected GPU: '{}' (coop_matrix={}, max_buffer={}MB)",
            gpu_info_name, gpu_coop, gpu_max_mb
        );

        // -------------------------------------------------------------------
        // Model build — the reader serves ALL tensor data from cache files.
        // Quantized matrices use web-rwkv's built-in quant cache disk layer.
        // Vectors and small FP16 matrices come from the CachedReader.
        // -------------------------------------------------------------------
        let _file_mb = file_len.unwrap_or_else(|| {
            std::fs::metadata(&model_path)
                .map(|m| m.len() / (1024 * 1024))
                .unwrap_or(0)
        });
        let num_emb = model_info.num_emb as u64;
        let num_layer = model_info.num_layer as u64;
        info!(
            "model: {}MB on disk, {} layers, {} emb",
            _file_mb, num_layer, num_emb
        );

        // Quantization plan
        let quant_spec_env = env::var("RWKV_QUANT").ok();
        let quant_layers: HashMap<usize, Quant> = if let Some(ref qs) = quant_spec_env {
            if qs == "none" {
                info!("quantization: none (user override)");
                HashMap::new()
            } else if let Some(n) = qs.strip_prefix("nf4=") {
                let n = n.parse::<usize>().unwrap_or(0).min(model_info.num_layer);
                if n > 0 && !gpu_coop {
                    warn!("NF4 requested but GPU lacks cooperative matrix");
                }
                let layers = (0..n).map(|l| (l, Quant::NF4)).collect();
                info!(
                    "quantization: NF4 {n} of {} layers (user override)",
                    model_info.num_layer
                );
                layers
            } else if let Ok(n) = qs.parse::<usize>() {
                let n = n.min(model_info.num_layer);
                let layers = (0..n).map(|l| (l, Quant::Int8)).collect();
                info!(
                    "quantization: Int8 {n} of {} layers (user override)",
                    model_info.num_layer
                );
                layers
            } else {
                // _model_data not needed for auto_quant when using mmap
                auto_quant(&model_info, &model_path, &[], gpu_coop, gpu_max_mb)
            }
        } else {
            auto_quant(&model_info, &model_path, &[], gpu_coop, gpu_max_mb)
        };

        info!("quantization: {} layers", quant_layers.len());
        std::fs::create_dir_all(&quant_cache_dir).ok();

        let builder = ModelBuilder::new(&context, model_reader)
            .quant(quant_layers)
            .quant_cache(quant_cache_dir.clone());

        let (runtime, state, initial_state) = match version {
            ModelVersion::V4 => {
                info!("building V4 model (may take several minutes)...");
                let m = builder.build_v4().await?;
                let b = web_rwkv::runtime::v4::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let init = s.init();
                let r = TokioRuntime::<web_rwkv::runtime::infer::Rnn>::new(b).await;
                (r, AnyState::V4(Box::new(s)), init)
            }
            ModelVersion::V5 => {
                info!("building V5 model (may take several minutes)...");
                let m = builder.build_v5().await?;
                let b = web_rwkv::runtime::v5::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let init = s.init();
                let r = TokioRuntime::<web_rwkv::runtime::infer::Rnn>::new(b).await;
                (r, AnyState::V5(Box::new(s)), init)
            }
            ModelVersion::V6 => {
                info!("building V6 model (may take several minutes)...");
                let m = builder.build_v6().await?;
                let b = web_rwkv::runtime::v6::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let init = s.init();
                let r = TokioRuntime::<web_rwkv::runtime::infer::Rnn>::new(b).await;
                (r, AnyState::V6(Box::new(s)), init)
            }
            ModelVersion::V7 => {
                info!("building V7 model (may take several minutes)...");
                let m = builder.build_v7().await?;
                info!("V7 model built successfully");
                let b = v7::Bundle::<f16>::new(m, 1);
                let s = b.state();
                let init = s.init();
                let r = TokioRuntime::<web_rwkv::runtime::infer::Rnn>::new(b).await;
                (r, AnyState::V7(Box::new(s)), init)
            }
        };

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

        // Build the cache index from the actual files on disk + saved metadata.
        // After build_v7(), web-rwkv's quant cache has saved _q.bin files for
        // quantized matrices. Our pre-build step saved _vec.bin for the rest.
        let mut cached_tensor_count = 0usize;
        let mut total_tensor_count = 0usize;
        let mut cache_tensors: Vec<CachedTensorInfo> = Vec::new();
        for (name, shape) in &tensor_metadata {
            total_tensor_count += 1;
            let safe = name.replace(['.', '/', '\\', ':'], "_");
            let q_path = quant_cache_dir.join(format!("{safe}_q.bin"));
            let vec_path = quant_cache_dir.join(format!("{safe}_vec.bin"));

            if q_path.exists() {
                // web-rwkv cached this as quantized — record it.
                // Keep the _vec.bin as fallback (load_matrix may need
                // raw FP16 data via tensor() if quant format mismatches).
                cache_tensors.push(CachedTensorInfo {
                    name: name.clone(),
                    dtype: "F16".to_string(),
                    shape: shape.clone(),
                    quantized: true,
                    md5: None,
                });
                cached_tensor_count += 1;
            } else if vec_path.exists() {
                cache_tensors.push(CachedTensorInfo {
                    name: name.clone(),
                    dtype: "F16".to_string(),
                    shape: shape.clone(),
                    quantized: false,
                    md5: None,
                });
                cached_tensor_count += 1;
            }
            // If neither file exists, this tensor wasn't cached.
            // On the next run, check_model_cache will detect this and fall
            // back to mmap. After build_v7 runs again, it'll be cached.
        }

        let cache_index = CacheIndex {
            model_name: std::path::Path::new(&model_path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            num_layer: model_info.num_layer,
            num_emb: model_info.num_emb,
            num_hidden: model_info.num_hidden,
            num_vocab: model_info.num_vocab,
            num_head: model_info.num_head,
            model_version: format!("{:?}", model_info.version),
            tensors: cache_tensors,
        };
        cache_index.save(&quant_cache_dir);
        info!(
            "cache index saved: {cached_tensor_count}/{total_tensor_count} tensors cached in {:?}",
            quant_cache_dir
        );

        let stop_token_sequences: Vec<Vec<u32>> = STOP_PATTERNS
            .iter()
            .filter_map(|s| tokenizer.encode(s.as_bytes()).ok())
            .collect();
        Ok(Self {
            context,
            runtime,
            state,
            initial_state,
            tokenizer,
            vocab_bytes,
            token_chunk_size,
            cancel: Arc::new(AtomicBool::new(false)),
            state_pool: HashMap::new(),
            session_lru: VecDeque::new(),
            max_sessions: 8,
            stop_token_sequences,
        })
    }

    /// Blend two session states element-wise: output = alpha * a + (1-alpha) * b
    pub fn blend_states(
        &mut self,
        session_a: String,
        session_b: String,
        alpha: f32,
        output_session: String,
    ) -> Result<(), EngineError> {
        let state_a = self
            .state_pool
            .get(&session_a)
            .and_then(|s| s.as_ref())
            .ok_or_else(|| EngineError::Backend(format!("session '{}' not found", session_a)))?;
        let state_b = self
            .state_pool
            .get(&session_b)
            .and_then(|s| s.as_ref())
            .ok_or_else(|| EngineError::Backend(format!("session '{}' not found", session_b)))?;

        if state_a.data().len() != state_b.data().len() {
            return Err(EngineError::Backend(
                "state tensors have different sizes".into(),
            ));
        }

        let blended: Vec<f32> = state_a
            .data()
            .iter()
            .zip(state_b.data().iter())
            .map(|(&a, &b)| alpha * a + (1.0 - alpha) * b)
            .collect();

        let blended_tensor = TensorCpu::from_data(state_a.shape(), blended)
            .map_err(|e| EngineError::Backend(format!("tensor creation failed: {e}")))?;

        // Store in state pool
        self.state_pool
            .insert(output_session.clone(), Some(blended_tensor));

        // Update LRU
        if let Some(pos) = self.session_lru.iter().position(|s| s == &output_session) {
            self.session_lru.remove(pos);
        }
        self.session_lru.push_back(output_session.clone());

        // Evict if over capacity
        while self.state_pool.len() > self.max_sessions {
            if let Some(oldest) = self.session_lru.pop_front() {
                self.state_pool.remove(&oldest);
                info!(session = oldest, "evicted session (LRU)");
            } else {
                break;
            }
        }

        info!(
            session_a = %session_a,
            session_b = %session_b,
            alpha = alpha,
            output_session = %output_session,
            "blended states"
        );

        Ok(())
    }

    /// Check whether appending `current` to `history` would complete any
    /// vocab-encoded stop sequence. Returns `true` if a stop pattern was
    /// detected.
    pub fn matches_stop_sequence(&self, history: &[u32], current: u32) -> bool {
        if self.stop_token_sequences.is_empty() {
            return false;
        }
        for seq in &self.stop_token_sequences {
            if seq.is_empty() {
                continue;
            }
            if seq[seq.len() - 1] != current {
                continue;
            }
            let needed = seq.len() - 1;
            if history.len() < needed {
                continue;
            }
            let start = history.len() - needed;
            if history[start..] == seq[..needed] {
                return true;
            }
        }
        false
    }

    pub async fn handle_complete(
        &mut self,
        req: CompleteReq,
        // Receiver half of the actor's mailbox. We carry it into
        // `handle_complete` so we can drain `Cancel` messages cooperatively
        // without round-tripping through the actor's main loop (the main
        // loop is blocked inside this call and cannot poll its own mailbox
        // until `handle_complete` returns).
        rx: &mut mpsc::Receiver<ActorMessage>,
    ) {
        let CompleteReq {
            prompt,
            prefill,
            max_tokens,
            temperature,
            top_a,
            on_token,
            grammar: _grammar,
            mut bnf_mask,
            seed,
            record_trace,
            state_id,
            save_as,
            reply,
            ..
        } = req;

        let mut seeded_rng: Option<StdRng> = seed.map(StdRng::seed_from_u64);

        let span = tracing::info_span!(
            "handle_complete",
            prompt_len = prompt.len(),
            max_tokens = max_tokens,
            temperature = temperature,
            seed = seed,
            state_id = state_id.as_deref().unwrap_or("none")
        );
        let _guard = span.enter();

        let outcome: Result<(String, TokenUsage, Vec<roco_engine::TokenTrace>), EngineError> =
            async {
                // Load cached state if specified.
                if let Some(ref sid) = state_id {
                    if let Some(pos) = self.session_lru.iter().position(|s| s == sid) {
                        self.session_lru.remove(pos);
                    }
                    self.session_lru.push_back(sid.clone());
                    match self.state_pool.get(sid) {
                        Some(Some(saved)) => {
                            self.state.load(saved.clone(), 0).map_err(|e| {
                                EngineError::Backend(format!("state load failed: {e}"))
                            })?;
                            info!(state = sid, "loaded cached state");
                        }
                        _ => {
                            self.state
                                .load(self.initial_state.clone(), 0)
                                .map_err(|e| {
                                    EngineError::Backend(format!("state reset failed: {e}"))
                                })?;
                            info!(state = sid, "cache miss — starting from blank");
                        }
                    }
                } else {
                    // No state specified: start from blank
                    self.state
                        .load(self.initial_state.clone(), 0)
                        .map_err(|e| EngineError::Backend(format!("state reset failed: {e}")))?;
                }

                // Grammar constraint — passed in as opaque Box<dyn BnfMask>.

                // inferd does NOT add any System/User/Assistant formatting.
                // The prompt text is used as-is — all formatting is the
                // caller's responsibility.
                let full = &prompt;

                // Build prefill tokens — no think suppression, caller controls content.
                let prefill_tokens = if let Some(pf) = prefill {
                    Some(
                        self.tokenizer
                            .encode(pf.as_bytes())
                            .map_err(|e| EngineError::Backend(format!("prefill tokenize: {e}")))?,
                    )
                } else {
                    None
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
                let top_a_val = top_a.unwrap_or(0.0);

                // Combine prompt tokens with prefill tokens if any
                let mut all_prompt_tokens = prompt_tokens;
                if let Some(pf) = prefill_tokens {
                    all_prompt_tokens.extend(pf);
                }
                let total_prompt_len = all_prompt_tokens.len();

                let mut inference = RnnInput::new(
                    vec![RnnInputBatch::new(all_prompt_tokens, RnnOption::Last)],
                    self.token_chunk_size,
                );

                let mut generated = Vec::new();
                let mut text = String::new();
                let mut first_token_sampled = false;
                let mut tokens_generated: usize = 0;
                let mut token_traces: Vec<roco_engine::TokenTrace> = Vec::new();

                // Flush prompt + sample first token
                //
                // This loop runs BEFORE the main generation loop and is NOT
                // bounded by `max_tokens` in the original code. As a result,
                // a bake call with `max_tokens: 1` (e.g. the FIM state-tune
                // bake) could still emit many tokens here before the main
                // loop ever starts counting. We now apply the cap uniformly:
                // either loop stops once `tokens_generated >= max_tokens`.
                loop {
                    if max_tokens > 0 && tokens_generated >= max_tokens {
                        break;
                    }
                    // Cooperative cancellation: drain any pending `Cancel` message so
                    // an interrupt (e.g. SSE client disconnect in `roco server`)
                    // actually stops a long-running generation instead of waiting
                    // for it to finish naturally at `max_tokens`. The actor's main
                    // loop is currently blocked inside this call, so it cannot poll
                    // its own mailbox; we do it here at every generation step.
                    while let Ok(ActorMessage::Cancel) = rx.try_recv() {
                        self.cancel.store(true, Ordering::Relaxed);
                    }
                    if self.cancel.load(Ordering::Relaxed) {
                        return Ok((
                            text,
                            TokenUsage {
                                prompt_tokens: total_prompt_len,
                                completion_tokens: generated.len(),
                            },
                            token_traces,
                        ));
                    }
                    let input = inference.clone();
                    let (input, output) = self
                        .runtime
                        .infer(input)
                        .await
                        .map_err(|e| EngineError::Backend(format!("RWKV inference: {e:?}")))?;
                    inference = input;

                    if !inference.batches[0].tokens.is_empty() {
                        continue;
                    }

                    // max_tokens=0: process prompt+prefill through inference to
                    // update the recurrent state (for state tuning/baking) but
                    // stop BEFORE sampling any generated tokens. This avoids
                    // contaminating the state with the model's own output.
                    if max_tokens == 0 {
                        break;
                    }

                    let ot = output[0].0.clone();
                    if ot.size() == 0 {
                        break;
                    }

                    let probs = softmax_one(
                        &self.context,
                        TensorCpu::from_data(ot.shape(), ot.to_vec())
                            .map_err(|e| EngineError::Backend(format!("tensor creation: {e}")))?,
                    )
                    .await
                    .map_err(|e| EngineError::Backend(format!("softmax: {e}")))?;

                    let mut p = probs.data().to_vec();

                    #[cfg(feature = "grammar")]
                    let token = {
                        if let Some(mask) = bnf_mask.as_mut() {
                            mask.mask(&mut p);
                            // Renormalize so grammar-constrained tokens have full probability mass
                            let sum: f32 = p.iter().filter(|&&v| v.is_finite()).sum();
                            if sum > 0.0 {
                                for v in p.iter_mut() {
                                    if v.is_finite() {
                                        *v /= sum;
                                    }
                                }
                            }
                            let t = sampling::sample_token_with_rng(
                                &p,
                                temperature,
                                1.0,
                                top_a_val,
                                seeded_rng.as_mut(),
                            );
                            if t > 0 {
                                mask.accept(t);
                                t
                            } else {
                                break;
                            }
                        } else {
                            sampling::sample_token_with_rng(
                                &p,
                                temperature,
                                top_p,
                                top_a_val,
                                seeded_rng.as_mut(),
                            )
                        }
                    };
                    #[cfg(not(feature = "grammar"))]
                    let token = sampling::sample_token_with_rng(
                        probs.data(),
                        temperature,
                        top_p,
                        top_a_val,
                        seeded_rng.as_mut(),
                    );

                    if token == 0 || token >= 65530 {
                        break;
                    }

                    let decoded = self
                        .tokenizer
                        .decode(&[token])
                        .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
                    let word = String::from_utf8_lossy(&decoded).to_string();

                    if let Some(ref cb) = on_token {
                        cb(&word);
                    }

                    // Stop conditions — check vocab-encoded token sequences so
                    // the model doesn't echo FIM scaffolding or continue a
                    // multi-turn dialogue in the prompt template.
                    if token == 10 || self.matches_stop_sequence(&generated, token) {
                        break;
                    }

                    if record_trace || std::env::var("ROCO_TRACE").is_ok() {
                        let prob = probs.data().get(token as usize).copied().unwrap_or(0.0);
                        let is_masked = bnf_mask.is_some();
                        if record_trace {
                            token_traces.push(roco_engine::TokenTrace {
                                token_id: token,
                                token_str: word.clone(),
                                probability: prob,
                                temperature,
                                top_p_cut: top_p,
                                grammar_masked: is_masked,
                                selected_by_grammar: is_masked,
                            });
                        }
                        tracing::debug!(
                            token_id = token,
                            word = %word,
                            prob = prob,
                            grammar_masked = is_masked,
                            "token_decode"
                        );
                    }

                    text.push_str(&word);
                    generated.push(token);
                    tokens_generated += 1;
                    first_token_sampled = true;
                    inference.batches[0].push(token);

                    // If the generated span picked up template markers, stop now.
                    if self.matches_stop_sequence(&generated, token) {
                        break;
                    }
                }

                if !first_token_sampled {
                    // Save state under caller-specified name (e.g. for baking:
                    // process prompt+prefill tokens with max_tokens=0, then
                    // save the resulting state for later use).
                    if let Some(ref sid) = save_as {
                        match self.state.back(0).await {
                            Ok(saved_state) => {
                                self.state_pool.insert(sid.clone(), Some(saved_state));
                                info!(state = sid, "saved baked state");
                            }
                            Err(e) => {
                                warn!(state = sid, error = %e, "failed to save state (silent)")
                            }
                        }
                    }
                    return Ok((
                        text,
                        TokenUsage {
                            prompt_tokens: prompt_len,
                            completion_tokens: 0,
                        },
                        token_traces,
                    ));
                }

                // Generate remaining tokens
                let mut fim_tokens_generated = 0usize;
                for _ in 1..max_tokens {
                    // Cooperative cancellation (see note above): drain any pending
                    // `Cancel` so an SSE-disconnect / Ctrl+C interrupt reaches the
                    // generation within one token of arriving instead of waiting
                    // for `max_tokens`.
                    while let Ok(ActorMessage::Cancel) = rx.try_recv() {
                        self.cancel.store(true, Ordering::Relaxed);
                    }
                    if self.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let input = inference.clone();
                    let (input, output) = self.runtime.infer(input).await.map_err(|e| {
                        EngineError::Backend(format!("RWKV inference (gen): {e:?}"))
                    })?;
                    inference = input;

                    let ot = output[0].0.clone();
                    if ot.size() == 0 {
                        break;
                    }

                    let probs = softmax_one(
                        &self.context,
                        TensorCpu::from_data(ot.shape(), ot.to_vec())
                            .map_err(|e| EngineError::Backend(format!("tensor creation: {e}")))?,
                    )
                    .await
                    .map_err(|e| EngineError::Backend(format!("softmax: {e}")))?;

                    #[cfg(feature = "grammar")]
                    let token_opt: Option<u32> = {
                        let mut p = probs.data().to_vec();
                        if let Some(mask) = bnf_mask.as_mut() {
                            mask.mask(&mut p);
                            // Renormalize so grammar-constrained tokens have full probability mass
                            let sum: f32 = p.iter().filter(|&&v| v.is_finite()).sum();
                            if sum > 0.0 {
                                for v in p.iter_mut() {
                                    if v.is_finite() {
                                        *v /= sum;
                                    }
                                }
                            }
                            let t = sampling::sample_token_with_rng(
                                &p,
                                temperature,
                                1.0,
                                top_a_val,
                                seeded_rng.as_mut(),
                            );
                            if t > 0 {
                                mask.accept(t);
                                Some(t)
                            } else {
                                None
                            }
                        } else {
                            Some(sampling::sample_token_with_rng(
                                &p,
                                temperature,
                                top_p,
                                top_a_val,
                                seeded_rng.as_mut(),
                            ))
                        }
                    };
                    #[cfg(not(feature = "grammar"))]
                    let token_opt: Option<u32> = Some(sampling::sample_token_with_rng(
                        probs.data(),
                        temperature,
                        top_p,
                        top_a_val,
                        seeded_rng.as_mut(),
                    ));

                    let token = match token_opt {
                        Some(t) => t,
                        None => break,
                    };

                    if token == 0 || token >= 65530 {
                        break;
                    }

                    let decoded = self
                        .tokenizer
                        .decode(&[token])
                        .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
                    let word = String::from_utf8_lossy(&decoded).to_string();

                    if let Some(ref cb) = on_token {
                        cb(&word);
                    }

                    // General stop conditions — must run on EVERY token (not just the
                    // first) or the model echoes the FIM template and loops. This is
                    // the guard that keeps a resumed/baked FIM session from repeating
                    // the BEFORE/AFTER/INSERT scaffolding.
                    if token == 10 || self.matches_stop_sequence(&generated, token) {
                        // Don't append the stop marker to the output.
                        break;
                    }

                    if record_trace {
                        let prob = probs.data().get(token as usize).copied().unwrap_or(0.0);
                        let is_masked = bnf_mask.is_some();
                        token_traces.push(roco_engine::TokenTrace {
                            token_id: token,
                            token_str: word.clone(),
                            probability: prob,
                            temperature,
                            top_p_cut: top_p,
                            grammar_masked: is_masked,
                            selected_by_grammar: is_masked,
                        });
                    }

                    text.push_str(&word);
                    generated.push(token);
                    inference.batches[0] = RnnInputBatch::new(vec![token], RnnOption::Last);

                    // FIM session handling: after generating a reasonable INSERT,
                    // force-feed token 0 (end-of-sequence) to properly terminate
                    // the recurrent state, then break.
                    if state_id.as_deref() == Some(FIM_SESSION_NAME) {
                        fim_tokens_generated += 1;
                        // Allow at least 8 tokens for the INSERT, max 64 before forcing end
                        if fim_tokens_generated >= 8 {
                            // Check if we've hit a natural stopping point (sentence end)
                            // OR if we detect the FIM template pattern looping
                            let is_template_loop = self.matches_stop_sequence(&generated, token);
                            let should_force_end = fim_tokens_generated >= 64
                                || word.ends_with('.')
                                || word.ends_with('?')
                                || word.ends_with('!')
                                || is_template_loop;
                            if should_force_end {
                                // Feed token 0 to the model to update recurrent state with EOS
                                let _ = self
                                    .runtime
                                    .infer(RnnInput::new(
                                        vec![RnnInputBatch::new(vec![0u32], RnnOption::Last)],
                                        self.token_chunk_size,
                                    ))
                                    .await;
                                break;
                            }
                        }
                    }
                }

                let result_text = if generated.is_empty() {
                    return Err(EngineError::EmptyResponse);
                } else {
                    let valid_tokens: Vec<u32> = generated
                        .iter()
                        .copied()
                        .filter(|&t| t > 0 && t < 65530)
                        .collect();
                    let decoded = self
                        .tokenizer
                        .decode(&valid_tokens)
                        .unwrap_or_else(|_| Vec::new());
                    String::from_utf8_lossy(&decoded).to_string()
                };

                // Save state under caller-specified name
                if let Some(ref sid) = save_as {
                    match self.state.back(0).await {
                        Ok(saved_state) => {
                            self.state_pool.insert(sid.clone(), Some(saved_state));
                            info!(state = sid, tokens = generated.len(), "saved state");
                        }
                        Err(e) => warn!(state = sid, error = %e, "failed to save state"),
                    }
                    while self.state_pool.len() > self.max_sessions {
                        if let Some(oldest) = self.session_lru.pop_front() {
                            if self.state_pool.contains_key(&oldest) {
                                self.state_pool.remove(&oldest);
                                info!(state = oldest, "evicted (LRU)");
                            }
                        } else {
                            break;
                        }
                    }
                }

                Ok((
                    result_text,
                    TokenUsage {
                        prompt_tokens: prompt_len,
                        completion_tokens: generated.len(),
                    },
                    token_traces,
                ))
            }
            .await;

        let _ = reply.send(outcome);
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<ActorMessage>) {
        use ActorMessage::*;
        while let Some(msg) = rx.recv().await {
            match msg {
                Complete(req) => {
                    self.cancel.store(false, Ordering::Relaxed);
                    let _result = self.handle_complete(req, &mut rx).await;
                    // reply is sent inside handle_complete
                }
                BlendStates(req) => {
                    let BlendReq {
                        session_a,
                        session_b,
                        alpha,
                        output_session,
                        reply,
                    } = req;
                    let result = self.blend_states(session_a, session_b, alpha, output_session);
                    let _ = reply.send(result);
                }
                Cancel => {
                    self.cancel.store(true, Ordering::Relaxed);
                }
                #[cfg(feature = "grammar")]
                GetVocabBytes(reply) => {
                    let _ = reply.send(self.vocab_bytes.clone());
                }
                SaveState(reply) => {
                    let result = self.save_state_bytes().await;
                    let _ = reply.send(result);
                }
                LoadState(bytes, reply) => {
                    let result = self.load_state_bytes(&bytes).await;
                    let _ = reply.send(result);
                }
                FeedEos(_state_name) => {
                    // Reset to blank initial state. In the new design,
                    // each generation loads its own state explicitly, so
                    // no per-session EOS is needed.
                    let _ = self
                        .state
                        .load(self.initial_state.clone(), 0);
                }
                Bake { state_id, text, name, reply } => {
                    // Load cached state if specified
                    if let Some(ref sid) = state_id {
                        if let Some(Some(saved)) = self.state_pool.get(sid) {
                            let _ = self.state.load(saved.clone(), 0);
                        } else {
                            let _ = self.state.load(self.initial_state.clone(), 0);
                        }
                    } else {
                        let _ = self.state.load(self.initial_state.clone(), 0);
                    }
                    // Tokenize and feed text through model
                    let tokens = self.tokenizer
                        .encode(text.as_bytes())
                        .map_err(|e| EngineError::Backend(format!("bake tokenize: {e}")));
                    match tokens {
                        Ok(tokens) => {
                            let _ = self
                                .runtime
                                .infer(RnnInput::new(
                                    vec![RnnInputBatch::new(tokens, RnnOption::Last)],
                                    self.token_chunk_size,
                                ))
                                .await;
                            // Save resulting state
                            if let Ok(tensor) = self.state.back(0).await {
                                self.state_pool.insert(name.clone(), Some(tensor));
                                info!(state = name, "bake complete");
                            }
                            let _ = reply.send(Ok(()));
                        }
                        Err(e) => {
                            let _ = reply.send(Err(e));
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// State (de)serialization — recurrent vector (incl. min-decay channels)
// ---------------------------------------------------------------------------

/// Layout: 4 × u32 little-endian dims (num_emb, head_size+2, num_layer, 1)
/// followed by f32 little-endian data in row-major order.
///
/// After the tensor data, we append a BLAKE3 hash (32 bytes) for corruption
/// detection, followed by a model metadata header:
///   - 4 bytes: model version string length (u32 LE)
///   - N bytes: model version string (e.g. "v7")
///   - 4 bytes: web-rwkv version string length (u32 LE)
///   - N bytes: web-rwkv version string
///   - 32 bytes: final BLAKE3 hash of all preceding bytes
fn serialize_state(t: &TensorCpu<f32>) -> Vec<u8> {
    let shape = t.shape();
    let data: Vec<f32> = t.data().iter().copied().collect();
    let mut out = Vec::with_capacity(16 + data.len() * 4 + 128);

    // Dims
    for d in shape.iter() {
        out.extend_from_slice(&(d as u32).to_le_bytes());
    }
    // Tensor data
    for x in data {
        out.extend_from_slice(&x.to_le_bytes());
    }

    // Model metadata for cross-version compatibility checks
    let model_version = env!("CARGO_PKG_VERSION", "0.1.0");
    let web_rwkv_version = "0.10.0"; // TODO: read from Cargo.toml at build time

    // Append model version string
    out.extend_from_slice(&(model_version.len() as u32).to_le_bytes());
    out.extend_from_slice(model_version.as_bytes());

    // Append web-rwkv version string
    out.extend_from_slice(&(web_rwkv_version.len() as u32).to_le_bytes());
    out.extend_from_slice(web_rwkv_version.as_bytes());

    // Final BLAKE3 hash of everything above
    let hash = blake3::hash(&out);
    out.extend_from_slice(hash.as_bytes());
    out
}

fn deserialize_state(bytes: &[u8]) -> Result<(Vec<u32>, Vec<f32>), EngineError> {
    // Minimum: 16 bytes (4×u32 dims) + 32 bytes (BLAKE3 hash).
    // Old format: no metadata, just 4 dims + data + 32 byte hash.
    // New format: 4 dims + data + metadata strings + 32 byte hash.
    if bytes.len() < 48 {
        return Err(EngineError::Backend("state bytes too short".into()));
    }

    // Separate hash from payload (last 32 bytes)
    let payload_len = bytes.len() - 32;
    let (payload, stored_hash) = bytes.split_at(payload_len);

    // Verify BLAKE3 hash
    let computed_hash = blake3::hash(payload);
    if computed_hash.as_bytes() != stored_hash {
        return Err(EngineError::Backend(
            "state checksum mismatch — data may be corrupted or from a different model version"
                .into(),
        ));
    }

    if (payload.len() - 16) % 4 != 0 {
        return Err(EngineError::Backend(
            "state bytes malformed (alignment)".into(),
        ));
    }

    let mut dims = [0u32; 4];
    for (i, d) in dims.iter_mut().enumerate() {
        *d = u32::from_le_bytes(payload[i * 4..i * 4 + 4].try_into().unwrap());
    }

    // Determine format: old (4 dims + data + hash) or new (4 dims + data + metadata + hash)
    // After 4 dims, the rest is tensor data. We need to find where metadata starts.
    // Metadata starts at a version-string-length marker that points past the tensor data.
    // Since we don't know the exact tensor size from the header alone, we scan for
    // a valid version string prefix in the last ~200 bytes.

    // Tensor data: starts at byte 16, ends before the metadata marker.
    // Simplified approach: just parse dims + all floats up to the metadata section.
    // The metadata section (if present) starts with a u32 length for the model version.
    // We walk backwards from the end of payload to find valid metadata.

    // Try to locate metadata: the last variable-length section before hash
    // has model_version_len (u32 LE) + model_version bytes + web_rwkv_len (u32 LE) + web_rwkv bytes.
    // Minimum metadata header: 2 × 4 bytes (lengths) + 2 × 1 byte (min version strings) = 10 bytes
    // We scan: look for a u32 at some position N bytes from end that is a plausible string length.

    // For simplicity, find the tensor data end by looking for the metadata marker.
    // The tensor data occupies bytes 16..payload_len-metadata_len-hash_len.
    // We try to read metadata from the end of payload backwards.

    let metadata_start = find_metadata_start(payload).unwrap_or(payload.len()); // No metadata found — old format
    let tensor_data = &payload[16..metadata_start];

    let n = tensor_data.len() / 4;
    let mut data = Vec::with_capacity(n);
    for i in 0..n {
        data.push(f32::from_le_bytes(
            tensor_data[i * 4..i * 4 + 4].try_into().unwrap(),
        ));
    }

    Ok((dims.to_vec(), data))
}

/// Scan backwards in `payload` to find the metadata section start position.
/// Returns `None` if no metadata is present (old format).
fn find_metadata_start(payload: &[u8]) -> Option<usize> {
    // Metadata section is at the end of the payload, before the 32-byte hash.
    // It has: model_version_len (u32 LE) + model_version + web_rwkv_len (u32 LE) + web_rwkv
    // We scan the last ~200 bytes looking for a plausible version string length.

    let end = payload.len();
    let search_start = end.saturating_sub(200);

    // Walk backwards looking for a valid model version string length marker
    // followed by the known model version prefix.
    for start in (search_start..end.saturating_sub(8)).rev() {
        let version_len =
            u32::from_le_bytes(payload[start..start + 4].try_into().unwrap()) as usize;
        // Plausible version string: 1-20 chars
        if version_len > 0 && version_len < 20 && start + 4 + version_len + 4 <= end {
            let version_end = start + 4 + version_len;
            let web_rwkv_len =
                u32::from_le_bytes(payload[version_end..version_end + 4].try_into().unwrap())
                    as usize;
            if web_rwkv_len > 0 && web_rwkv_len < 20 && version_end + 4 + web_rwkv_len <= end {
                // Found plausible metadata
                return Some(start);
            }
        }
    }
    None
}

impl RwkvActor {
    async fn save_state_bytes(&self) -> Result<Vec<u8>, EngineError> {
        let t = self
            .state
            .back(0)
            .await
            .map_err(|e| EngineError::Backend(format!("state back: {e:?}")))?;
        Ok(serialize_state(&t))
    }

    async fn load_state_bytes(&self, bytes: &[u8]) -> Result<(), EngineError> {
        let (dims, data) = deserialize_state(bytes)?;
        let shape = [
            dims[0] as usize,
            dims[1] as usize,
            dims[2] as usize,
            dims[3] as usize,
        ];
        let tensor = TensorCpu::from_data(shape, data)
            .map_err(|e| EngineError::Backend(format!("state from_data: {e:?}")))?;
        self.state
            .load(tensor, 0)
            .map_err(|e| EngineError::Backend(format!("state load: {e:?}")))?;
        Ok(())
    }
}
