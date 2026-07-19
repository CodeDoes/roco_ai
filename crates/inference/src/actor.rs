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
use web_rwkv::runtime::model::State as RwkvState;
use roco_engine::{BnfMask, EngineError, TokenUsage};
use safetensors::SafeTensors;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};
use web_rwkv::context::{Context, ContextBuilder};
use web_rwkv::runtime::infer::{Rnn, RnnInput, RnnInputBatch, RnnOption};
use web_rwkv::runtime::loader::Loader;
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

use crate::config::{auto_quant, get_pipeline_cache_path, get_quant_cache_dir, default_model_path};
use crate::sampling;

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
// Request / message types
// ---------------------------------------------------------------------------

pub struct CompleteReq {
    pub system: String,
    pub prompt: String,
    pub prefill: Option<String>,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_a: Option<f32>,
    #[cfg_attr(not(feature = "grammar"), allow(dead_code))]
    pub grammar: Option<String>,
    /// Opaque grammar constraint callback, created outside this crate
    /// so grammar-engine types never enter this compilation unit.
    #[cfg_attr(not(feature = "grammar"), allow(dead_code))]
    pub bnf_mask: Option<Box<dyn BnfMask>>,
    pub reply: oneshot::Sender<Result<(String, TokenUsage), EngineError>>,
    pub preserve_state: bool,
    pub on_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
    pub session: Option<String>,
    /// Wall-clock deadline for the entire completion in milliseconds.
    /// 0 = no deadline.
    pub deadline_ms: u64,
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
    /// Serialize the current recurrent state (incl. the per-head min-decay
    /// channels) to bytes for downstream introspection / monitoring.
    SaveState(oneshot::Sender<Result<Vec<u8>, EngineError>>),
    /// Restore a recurrent state previously produced by `SaveState`.
    LoadState(Vec<u8>, oneshot::Sender<Result<(), EngineError>>),
}

impl From<CompleteReq> for ActorMessage {
    fn from(req: CompleteReq) -> Self { Self::Complete(req) }
}

impl From<BlendReq> for ActorMessage {
    fn from(req: BlendReq) -> Self { Self::BlendStates(req) }
}

// Session name used for FIM (fill-in-the-middle) state-tuning.
// When this session is active, the model has few-shot FIM examples baked
// into its recurrent state. After generating the INSERT completion, we
// must force-terminate with token 0 to prevent the model from continuing
// the few-shot dialogue pattern.
pub const FIM_SESSION_NAME: &str = "roco_fim";

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
    #[cfg_attr(not(feature = "grammar"), allow(dead_code))]
    pub vocab_bytes: Vec<Vec<u8>>,
    pub token_chunk_size: usize,
    pub _model_data: Vec<u8>,
    pub cancel: Arc<AtomicBool>,
    pub state_pool: HashMap<String, Option<TensorCpu<f32>>>,
    pub session_lru: VecDeque<String>,
    pub max_sessions: usize,
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
            let candidates = ["assets/vocab/rwkv_vocab_v20230424.json", "models/rwkv_vocab_v20230424.json"];
            for c in &candidates {
                let p = dir.join(c);
                if p.exists() { return p.to_string_lossy().to_string(); }
            }
            dir.join(candidates[0]).to_string_lossy().to_string()
        });
        let token_chunk_size: usize = env::var("RWKV_CHUNK").ok().and_then(|s| s.parse().ok()).unwrap_or(128);

        info!(model_path = %model_path, vocab_path = %vocab_path, "loading RWKV model");
        let vocab_text = tokio::fs::read_to_string(&vocab_path).await?;
        let tokenizer = Tokenizer::new(&vocab_text)?;
        info!("tokenizer loaded");
        let vocab_bytes = tokenizer.token_index_to_bytes().to_vec();

        let model_data = std::fs::read(&model_path)?;
        let model = SafeTensors::deserialize(&model_data)?;
        let info = Loader::info(&model)?;
        let version = info.version;
        info!(version = ?version, layers = info.num_layer, vocab = info.num_vocab, emb = info.num_emb, "model info");

        // GPU enumeration
        let instance = wgpu::Instance::default();
        let all_adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
        let mut scored: Vec<_> = all_adapters.into_iter()
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
                let coop_bonus = if coop { 100 } else { 0 };
                info!("  [{}] {} | type={:?} | coop_matrix={} | max_buffer={}MB | backend={:?}",
                    if coop { "✓" } else { "✗" }, i.name, i.device_type, coop, max_buf_mb, i.backend);
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
            let coop = adapter.features().contains(wgpu::Features::EXPERIMENTAL_COOPERATIVE_MATRIX);
            let max_mb = adapter.limits().max_buffer_size / (1024 * 1024);
            if let Some(ref filter) = adapter_name_filter {
                if !ainfo.name.to_lowercase().contains(&filter.to_lowercase()) { continue; }
            }
            info!("trying adapter: '{}' (type={:?}, coop={}, {}MB)", ainfo.name, ainfo.device_type, coop, max_mb);

            let cache_path = get_pipeline_cache_path(&model_path);
            let cached_pipelines = std::fs::read(&cache_path).ok();
            let mut builder = ContextBuilder::new(adapter).auto_limits(&info);
            if let Some(ref data) = cached_pipelines { builder = builder.with_pipeline_cache(data.clone()); }
            match builder.build().await {
                Ok(ctx) => {
                    info!("context created on: '{}'", ainfo.name);
                    context = Some(ctx); gpu_coop = coop; gpu_max_mb = max_mb; gpu_info_name = ainfo.name;
                    break;
                }
                Err(e) => warn!("adapter '{}' failed: {}", ainfo.name, e),
            }
        }

        let context = context.ok_or_else(|| anyhow::anyhow!("no adapter could create a WebGPU context (tried {} adapters)", adapter_count))?;
        info!("selected GPU: '{}' (coop_matrix={}, max_buffer={}MB)", gpu_info_name, gpu_coop, gpu_max_mb);

        // Memory estimate
        let num_emb = info.num_emb as u64;
        let num_layer = info.num_layer as u64;
        let num_vocab = info.num_vocab as u64;
        let resident_fp16_mb = (2 * num_emb * num_vocab * 2 + num_emb * num_layer * 4 * 4) / (1024 * 1024);
        let file_mb = std::fs::metadata(&model_path).map(|m| m.len() / (1024 * 1024)).unwrap_or(0);
        info!("model memory: file={}MB resident(FP16)={}MB layers={} emb={} vocab={}", file_mb, resident_fp16_mb, num_layer, num_emb, num_vocab);

        // Quantization
        let quant_spec_env = env::var("RWKV_QUANT").ok();
        let quant_layers: HashMap<usize, Quant> = if let Some(ref qs) = quant_spec_env {
            if qs == "none" { info!("quantization: none (user override)"); HashMap::new() }
            else if let Some(n) = qs.strip_prefix("nf4=") {
                let n = n.parse::<usize>().unwrap_or(0).min(info.num_layer);
                if n > 0 && !gpu_coop { warn!("NF4 requested but GPU lacks cooperative matrix"); }
                let layers = (0..n).map(|l| (l, Quant::NF4)).collect();
                info!("quantization: NF4 {n} of {} layers (user override)", info.num_layer);
                layers
            } else if let Ok(n) = qs.parse::<usize>() {
                let n = n.min(info.num_layer);
                let layers = (0..n).map(|l| (l, Quant::Int8)).collect();
                info!("quantization: Int8 {n} of {} layers (user override)", info.num_layer);
                layers
            } else { auto_quant(&info, &model_path, &model_data, gpu_coop, gpu_max_mb) }
        } else { auto_quant(&info, &model_path, &model_data, gpu_coop, gpu_max_mb) };

        info!("quantization: {} layers", quant_layers.len());
        let quant_cache_dir = get_quant_cache_dir(&model_path);
        std::fs::create_dir_all(&quant_cache_dir).ok();
        let builder = ModelBuilder::new(&context, model).quant(quant_layers).quant_cache(quant_cache_dir);

        #[cfg(debug_assertions)]
        warn!("Debug build detected! build_v7() may hang on some GPUs. Rebuild with `--release`.");

        let (runtime, state, initial_state) = match version {
            ModelVersion::V4 => {
                let m = builder.build_v4().await?;
                let b = web_rwkv::runtime::v4::Bundle::<f16>::new(m, 1);
                let s = b.state(); let init = s.init(); let r = TokioRuntime::new(b).await;
                (r, AnyState::V4(Box::new(s)), init)
            }
            ModelVersion::V5 => {
                let m = builder.build_v5().await?;
                let b = web_rwkv::runtime::v5::Bundle::<f16>::new(m, 1);
                let s = b.state(); let init = s.init(); let r = TokioRuntime::new(b).await;
                (r, AnyState::V5(Box::new(s)), init)
            }
            ModelVersion::V6 => {
                let m = builder.build_v6().await?;
                let b = web_rwkv::runtime::v6::Bundle::<f16>::new(m, 1);
                let s = b.state(); let init = s.init(); let r = TokioRuntime::new(b).await;
                (r, AnyState::V6(Box::new(s)), init)
            }
            ModelVersion::V7 => {
                let m = builder.build_v7().await?;
                let b = v7::Bundle::<f16>::new(m, 1);
                let s = b.state(); let init = s.init(); let r = TokioRuntime::new(b).await;
                (r, AnyState::V7(Box::new(s)), init)
            }
        };

        if let Some(data) = context.get_pipeline_cache_data() {
            let cache_path = get_pipeline_cache_path(&model_path);
            if let Some(parent) = cache_path.parent() { std::fs::create_dir_all(parent).ok(); }
            match std::fs::write(&cache_path, &data) {
                Ok(()) => info!(path = ?cache_path, size = data.len(), "saved pipeline cache"),
                Err(e) => warn!(path = ?cache_path, error = %e, "failed to save pipeline cache"),
            }
        }

        info!("RWKV runtime initialized");

        Ok(Self {
            context, runtime, state, initial_state, tokenizer,
            vocab_bytes, token_chunk_size, _model_data: model_data,
            cancel: Arc::new(AtomicBool::new(false)),
            state_pool: HashMap::new(), session_lru: VecDeque::new(), max_sessions: 8,
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
        let state_a = self.state_pool.get(&session_a)
            .and_then(|s| s.as_ref())
            .ok_or_else(|| EngineError::Backend(format!("session '{}' not found", session_a)))?;
        let state_b = self.state_pool.get(&session_b)
            .and_then(|s| s.as_ref())
            .ok_or_else(|| EngineError::Backend(format!("session '{}' not found", session_b)))?;

        if state_a.data().len() != state_b.data().len() {
            return Err(EngineError::Backend("state tensors have different sizes".into()));
        }

        let blended: Vec<f32> = state_a.data().iter()
            .zip(state_b.data().iter())
            .map(|(&a, &b)| alpha * a + (1.0 - alpha) * b)
            .collect();

        let blended_tensor = TensorCpu::from_data(state_a.shape(), blended)
            .map_err(|e| EngineError::Backend(format!("tensor creation failed: {e}")))?;

        // Store in state pool
        self.state_pool.insert(output_session.clone(), Some(blended_tensor));
        
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

    pub async fn handle_complete(
        &mut self,
        system: String,
        prompt: String,
        prefill: Option<String>,
        max_tokens: usize,
        temperature: f32,
        top_a: Option<f32>,
        preserve_state: bool,
        on_token: Option<Box<dyn Fn(&str) + Send + Sync>>,
        #[cfg_attr(not(feature = "grammar"), allow(unused))]
        grammar: Option<String>,
        session: Option<String>,
        mut bnf_mask: Option<Box<dyn BnfMask>>,
        // Receiver half of the actor's mailbox. We carry it into
        // `handle_complete` so we can drain `Cancel` messages cooperatively
        // without round-tripping through the actor's main loop (the main
        // loop is blocked inside this call and cannot poll its own mailbox
        // until `handle_complete` returns).
        rx: &mut mpsc::Receiver<ActorMessage>,
        // Wall-clock deadline for the entire completion in milliseconds.
        // 0 = no deadline. The backend already wraps `complete()` in
        // `tokio::time::timeout` and posts a `Cancel` on expiry; we keep
        // this independent hard-stop so even if the Cancel somehow does
        // not propagate (backend dropped sender, panic, etc.) we still
        // cap runaway generations on the actor side.
        #[allow(unused)] deadline_ms: u64,
    ) -> Result<(String, TokenUsage), EngineError> {
        let session_id = session.as_ref().cloned();
        let is_fim_session = session_id.as_deref() == Some(FIM_SESSION_NAME);

        // Load session state or reset
        if let Some(ref sid) = session_id {
            if let Some(pos) = self.session_lru.iter().position(|s| s == sid) { self.session_lru.remove(pos); }
            self.session_lru.push_back(sid.clone());
            match self.state_pool.get(sid) {
                Some(Some(saved)) => {
                    self.state.load(saved.clone(), 0)
                        .map_err(|e| EngineError::Backend(format!("session load failed: {e}")))?;
                    info!(session = sid, "loaded session state");
                }
                _ => {
                    self.state.load(self.initial_state.clone(), 0)
                        .map_err(|e| EngineError::Backend(format!("state reset failed: {e}")))?;
                    info!(session = sid, "new session (blank state)");
                }
            }
        } else if !preserve_state {
            self.state.load(self.initial_state.clone(), 0)
                .map_err(|e| EngineError::Backend(format!("state reset failed: {e}")))?;
        }

        // Grammar constraint — passed in as opaque Box<dyn BnfMask>.
        // Cannot be created here — kbnf types would overflow the compiler
        // (E0275 against web-rwkv's TokioRuntime). The application layer
        // builds the BnfMask from grammar + vocab_bytes and passes it in.

        // Build prompt.
        //
        // When resuming a baked session (session_id.is_some()) the system
        // prompt and few-shot context are already encoded in the recurrent
        // state — re-prepending `System:` would double-feed it and make the
        // model echo the scaffolding. So resumed calls feed only the
        // incremental context. The bake call itself (preserve_state) also
        // skips the System wrapper for the same reason.
        let full = if system.is_empty() {
            format!("User: {prompt}\n\nAssistant:")
        } else if session_id.is_some() || preserve_state {
            format!("User: {prompt}\n\nAssistant:")
        } else {
            format!("System: {}\n\nUser: {prompt}\n\nAssistant:", system.trim())
        };

        // Pre-fill if provided (for pre-think blocks, etc.)
        let prefill_tokens = if let Some(pf) = prefill {
            Some(self.tokenizer.encode(pf.as_bytes())
                .map_err(|e| EngineError::Backend(format!("prefill tokenize: {e}")))?)
        } else {
            None
        };

        let prompt_tokens = self.tokenizer.encode(full.as_bytes())
            .map_err(|e| EngineError::Backend(format!("tokenizer encode: {e}")))?;
        let prompt_len = prompt_tokens.len();

        let top_p = if temperature < 0.3 { 0.8 } else if temperature < 0.7 { 0.9 } else { 0.95 };
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
                return Ok((text, TokenUsage { prompt_tokens: total_prompt_len, completion_tokens: generated.len() }));
            }
            let input = inference.clone();
            let (input, output) = self.runtime.infer(input).await
                .map_err(|e| EngineError::Backend(format!("RWKV inference: {e:?}")))?;
            inference = input;

            if inference.batches[0].tokens.len() > 0 { continue; }

            let ot = output[0].0.clone();
            if ot.size() == 0 { break; }

            let probs = softmax_one(&self.context, TensorCpu::from_data(ot.shape(), ot.to_vec())
                .map_err(|e| EngineError::Backend(format!("tensor creation: {e}")))?)
                .await.map_err(|e| EngineError::Backend(format!("softmax: {e}")))?;

            let mut p = probs.data().to_vec();

            #[cfg(feature = "grammar")]
            let token = {
                if let Some(mask) = bnf_mask.as_mut() {
                    mask.mask(&mut p);
                    // Renormalize so grammar-constrained tokens have full probability mass
                    let sum: f32 = p.iter().filter(|&&v| v.is_finite()).sum();
                    if sum > 0.0 {
                        for v in p.iter_mut() { if v.is_finite() { *v /= sum; } }
                    }
                    let t = sampling::sample_token(&p, temperature, 1.0, top_a_val);
                    if t > 0 {
                        mask.accept(t);
                        t
                    } else { break }
                } else { sampling::sample_token(&p, temperature, top_p, top_a_val) }
            };
            #[cfg(not(feature = "grammar"))]
            let token = sampling::sample_token(probs.data(), temperature, top_p, top_a_val);

            if token == 0 { break; }

            let decoded = self.tokenizer.decode(&[token])
                .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
            let word = String::from_utf8_lossy(&decoded).to_string();

            if let Some(ref cb) = on_token { cb(&word); }

            // Stop conditions - catch FIM template pattern variations so the
            // model doesn't echo the BEFORE/AFTER/INSERT scaffolding.
            let potential_text = format!("{}{}", text, word);
            let is_stop = word == "\n\n"
                || word.trim() == "User:"
                || word.trim() == "Human:"
                || word.trim() == "NOW"
                || word.trim_start().starts_with("BEFORE:")
                || word.trim_start().starts_with("AFTER:")
                || word.trim_start().starts_with("INSERT:")
                || potential_text.contains("User: NOW")
                || potential_text.contains("BEFORE:")
                || potential_text.contains("AFTER:")
                || potential_text.contains("INSERT:");
            if is_stop { break; }

            text.push_str(&word);
            generated.push(token);
            tokens_generated += 1;
            first_token_sampled = true;
            inference.batches[0].push(token);

            // If the generated span picked up template markers, stop now.
            if text.contains("User: NOW") || text.contains("BEFORE:") || text.contains("AFTER:") || text.contains("INSERT:") {
                break;
            }
        }

        if !first_token_sampled {
            return Ok((text, TokenUsage { prompt_tokens: prompt_len, completion_tokens: 0 }));
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
            if self.cancel.load(Ordering::Relaxed) { break; }
            let input = inference.clone();
            let (input, output) = self.runtime.infer(input).await
                .map_err(|e| EngineError::Backend(format!("RWKV inference (gen): {e:?}")))?;
            inference = input;

            let ot = output[0].0.clone();
            if ot.size() == 0 { break; }

            let probs = softmax_one(&self.context, TensorCpu::from_data(ot.shape(), ot.to_vec())
                .map_err(|e| EngineError::Backend(format!("tensor creation: {e}")))?)
                .await.map_err(|e| EngineError::Backend(format!("softmax: {e}")))?;

            #[cfg(feature = "grammar")]
            let token_opt: Option<u32> = {
                let mut p = probs.data().to_vec();
                if let Some(mask) = bnf_mask.as_mut() {
                    mask.mask(&mut p);
                    // Renormalize so grammar-constrained tokens have full probability mass
                    let sum: f32 = p.iter().filter(|&&v| v.is_finite()).sum();
                    if sum > 0.0 {
                        for v in p.iter_mut() { if v.is_finite() { *v /= sum; } }
                    }
                    let t = sampling::sample_token(&p, temperature, 1.0, top_a_val);
                    if t > 0 {
                        mask.accept(t);
                        Some(t)
                    } else { None }
                } else { Some(sampling::sample_token(&p, temperature, top_p, top_a_val)) }
            };
            #[cfg(not(feature = "grammar"))]
            let token_opt: Option<u32> = Some(sampling::sample_token(probs.data(), temperature, top_p, top_a_val));

            let token = match token_opt { Some(t) => t, None => break };

            if token == 0 { break; }

            let decoded = self.tokenizer.decode(&[token])
                .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
            let word = String::from_utf8_lossy(&decoded).to_string();

            if let Some(ref cb) = on_token { cb(&word); }

            // General stop conditions — must run on EVERY token (not just the
            // first) or the model echoes the FIM template and loops. This is
            // the guard that keeps a resumed/baked FIM session from repeating
            // the BEFORE/AFTER/INSERT scaffolding.
            let potential_text = format!("{}{}", text, word);
            let is_stop = word == "\n\n"
                || word.trim() == "User:"
                || word.trim() == "Human:"
                || word.trim() == "NOW"
                || word.trim_start().starts_with("BEFORE:")
                || word.trim_start().starts_with("AFTER:")
                || word.trim_start().starts_with("INSERT:")
                || potential_text.contains("User: NOW")
                || potential_text.contains("BEFORE:")
                || potential_text.contains("AFTER:")
                || potential_text.contains("INSERT:");
            if is_stop {
                // Don't append the stop marker to the output.
                break;
            }

            text.push_str(&word);
            generated.push(token);
            inference.batches[0] = RnnInputBatch::new(vec![token], RnnOption::Last);

            // FIM session handling: after generating a reasonable INSERT,
            // force-feed token 0 (end-of-sequence) to properly terminate
            // the recurrent state, then break.
            if is_fim_session {
                fim_tokens_generated += 1;
                // Allow at least 8 tokens for the INSERT, max 64 before forcing end
                if fim_tokens_generated >= 8 {
                    // Check if we've hit a natural stopping point (sentence end)
                    // OR if we detect the FIM template pattern looping
                    let is_template_loop = text.contains("User: NOW")
                        || text.contains("BEFORE:")
                        || text.contains("AFTER:")
                        || text.contains("INSERT:");
                    let should_force_end = fim_tokens_generated >= 64
                        || word.ends_with('.')
                        || word.ends_with('?')
                        || word.ends_with('!')
                        || is_template_loop;
                    if should_force_end {
                        // Feed token 0 to the model to update recurrent state with EOS
                        let _ = self.runtime.infer(RnnInput::new(
                            vec![RnnInputBatch::new(vec![0u32], RnnOption::Last)],
                            self.token_chunk_size,
                        )).await;
                        break;
                    }
                }
            }
        }

        let result_text = if generated.is_empty() {
            return Err(EngineError::EmptyResponse);
        } else {
            let decoded = self.tokenizer.decode(&generated)
                .map_err(|e| EngineError::Backend(format!("tokenizer decode: {e}")))?;
            String::from_utf8_lossy(&decoded).to_string()
        };

        // Save session state
        if let Some(ref sid) = session_id {
            match self.state.back(0).await {
                Ok(saved_state) => {
                    self.state_pool.insert(sid.clone(), Some(saved_state));
                    info!(session = sid, tokens = generated.len(), "saved session state");
                }
                Err(e) => warn!(session = sid, error = %e, "failed to save session state"),
            }
            while self.state_pool.len() > self.max_sessions {
                if let Some(oldest) = self.session_lru.pop_front() {
                    self.state_pool.remove(&oldest);
                    info!(session = oldest, "evicted session (LRU)");
                } else { break; }
            }
        }

        Ok((result_text, TokenUsage { prompt_tokens: prompt_len, completion_tokens: generated.len() }))
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<ActorMessage>) {
        use ActorMessage::*;
        while let Some(msg) = rx.recv().await {
            match msg {
                Complete(req) => {
                    self.cancel.store(false, Ordering::Relaxed);
                    let CompleteReq {
                        system,
                        prompt,
                        prefill,
                        max_tokens,
                        temperature,
                        top_a,
                        grammar,
                        bnf_mask,
                        reply,
                        preserve_state,
                        on_token,
                        session,
                        deadline_ms,
                    } = req;
                    let result = self
                        .handle_complete(
                            system, prompt, prefill, max_tokens, temperature,
                            top_a, preserve_state, on_token, grammar, session,
                            bnf_mask,
                            &mut rx,
                            deadline_ms,
                        )
                        .await;
                    let _ = reply.send(result);
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
                Cancel => { self.cancel.store(true, Ordering::Relaxed); }
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
            }
        }
    }
}

// ---------------------------------------------------------------------------
// State (de)serialization — recurrent vector (incl. min-decay channels)
// ---------------------------------------------------------------------------

/// Layout: 4 × u32 little-endian dims (num_emb, head_size+2, num_layer, 1)
/// followed by f32 little-endian data in row-major order.
fn serialize_state(t: &TensorCpu<f32>) -> Vec<u8> {
    let shape = t.shape();
    let data: Vec<f32> = t.data().iter().copied().collect();
    let mut out = Vec::with_capacity(16 + data.len() * 4);
    for d in shape.iter() {
        out.extend_from_slice(&(d as u32).to_le_bytes());
    }
    for x in data {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn deserialize_state(bytes: &[u8]) -> Result<(Vec<u32>, Vec<f32>), EngineError> {
    if bytes.len() < 16 || (bytes.len() - 16) % 4 != 0 {
        return Err(EngineError::Backend("state bytes malformed".into()));
    }
    let mut dims = [0u32; 4];
    for (i, d) in dims.iter_mut().enumerate() {
        *d = u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    }
    let tail = &bytes[16..];
    let n = tail.len() / 4;
    let mut data = Vec::with_capacity(n);
    for i in 0..n {
        data.push(f32::from_le_bytes(tail[i * 4..i * 4 + 4].try_into().unwrap()));
    }
    Ok((dims.to_vec(), data))
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
