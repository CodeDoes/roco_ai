//! Unified inference system — load anything, run anywhere.
//!
//! Combines multiple backends (web-rwkv, candle, LiteRT, whisper.cpp, etc.)
//! under a single trait + registry. Auto-detects hardware, estimates
//! performance, manages model lifecycle, and routes tasks to the best engine.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    InferenceRegistry                        │
//! │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
//! │  │  RWKV    │  │  Candle  │  │  LiteRT  │  │ Whisper  │ ...│
//! │  │ (WGPU)   │  │ (WGPU)   │  │ (CPU/GPU)│  │ (CPU)    │    │
//! │  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │
//! │         │              │             │            │         │
//! │         ▼              ▼             ▼            ▼         │
//! │  ┌──────────────────────────────────────────────────────┐   │
//! │  │              Hardware Detector + Router              │   │
//! │  │  · Query GPU/CPU/RAM                                 │   │
//! │  │  · Estimate: does model fit? where? how fast?        │   │
//! │  │  · Route task → best engine based on model + hw      │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                                                             │
//! │  Model lifecycle: load ↔ warm pool ↔ unload (LRU eviction) │
//! │  Lock file protocol: /tmp/roco-infer/<model-id>.lock        │
//! └─────────────────────────────────────────────────────────────┘
//! ```

pub mod downloader;
pub mod hardware;
pub mod performance;

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::info;

use self::hardware::HardwareCapabilities;
use self::performance::{format_profile, known_profiles, ModelArch, PerformanceProfile};
use crate::engine::{CompletionRequest, TokenUsage};

// ---------------------------------------------------------------------------
// Inference input/output — unified across all model types
// ---------------------------------------------------------------------------

/// What you send to any model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceInput {
    /// Text generation (standard completion/chat)
    Text {
        system: String,
        prompt: String,
        max_tokens: usize,
        temperature: f32,
    },
    /// Speech-to-text (audio bytes)
    Audio {
        /// WAV/MP3 bytes
        data: Vec<u8>,
        /// Sample rate (Hz)
        sample_rate: u32,
    },
    /// Text-to-speech
    TextToSpeech {
        text: String,
        voice: Option<String>,
    },
    /// Image understanding (VLM)
    Image {
        /// JPEG/PNG bytes
        data: Vec<u8>,
        prompt: String,
    },
    /// Image generation (diffusion)
    TextToImage {
        prompt: String,
        width: u32,
        height: u32,
        steps: u32,
    },
    /// Embeddings
    Embed {
        texts: Vec<String>,
    },
    /// Classification
    Classify {
        text: String,
        labels: Vec<String>,
    },
}

/// What any model returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InferenceOutput {
    Text {
        text: String,
        usage: TokenUsage,
    },
    Audio {
        /// WAV bytes
        data: Vec<u8>,
        sample_rate: u32,
    },
    Image {
        /// PNG/JPEG bytes
        data: Vec<u8>,
    },
    Embeddings {
        vectors: Vec<Vec<f32>>,
    },
    Classification {
        label: String,
        score: f32,
        all_scores: Vec<(String, f32)>,
    },
}

// ---------------------------------------------------------------------------
// Engine trait — every backend implements this
// ---------------------------------------------------------------------------

/// A single inference engine (backend for a specific model type).
#[async_trait]
pub trait InferenceEngine: Send + Sync {
    /// Human-readable engine name.
    fn name(&self) -> &str;

    /// What model types this engine can handle.
    fn supported_archs(&self) -> Vec<ModelArch>;

    /// Hardware this engine prefers.
    fn preferred_hardware(&self) -> &str; // "gpu", "cpu", "hybrid"

    /// Load the model into memory.
    async fn load(&mut self) -> Result<(), InferenceError>;

    /// Unload the model, freeing memory.
    async fn unload(&mut self) -> Result<(), InferenceError>;

    /// Check if the model is currently loaded.
    fn is_loaded(&self) -> bool;

    /// Run inference.
    async fn infer(&self, input: InferenceInput) -> Result<InferenceOutput, InferenceError>;

    /// Estimated VRAM usage when loaded (MB).
    fn estimated_vram_mb(&self) -> u64;

    /// Estimated RAM usage when loaded (MB).
    fn estimated_ram_mb(&self) -> u64;
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, thiserror::Error)]
pub enum InferenceError {
    #[error("model not loaded: {0}")]
    NotLoaded(String),
    #[error("unsupported input type for engine {engine}: {input_type}")]
    UnsupportedInput {
        engine: String,
        input_type: String,
    },
    #[error("engine error: {0}")]
    Engine(String),
    #[error("out of memory: need {needed}MB but only {available}MB available")]
    OutOfMemory { needed: u64, available: u64 },
    #[error("hardware not supported: {0}")]
    Hardware(String),
}

// ---------------------------------------------------------------------------
// Adapter: wrap existing ModelBackend into InferenceEngine
// ---------------------------------------------------------------------------

/// Wraps a `ModelBackend` (from engine.rs) as an `InferenceEngine`.
/// This lets us use the existing RWKV backend, MockBackend, HTTP backends, etc.
#[allow(dead_code)]
pub struct BackendAdapter<B> {
    name: String,
    backend: B,
    arch: ModelArch,
    params_b: f64,
    quant_bits: u32,
    loaded: bool,
    vram_mb: u64,
    ram_mb: u64,
}

impl<B: crate::engine::ModelBackend + Send + Sync + 'static> BackendAdapter<B> {
    pub fn new(
        name: impl Into<String>,
        backend: B,
        arch: ModelArch,
        params_b: f64,
        quant_bits: u32,
        vram_mb: u64,
        ram_mb: u64,
    ) -> Self {
        Self {
            name: name.into(),
            backend,
            arch,
            params_b,
            quant_bits,
            loaded: true,
            vram_mb,
            ram_mb,
        }
    }
}

#[async_trait]
impl<B: crate::engine::ModelBackend + Send + Sync + 'static> InferenceEngine for BackendAdapter<B> {
    fn name(&self) -> &str {
        &self.name
    }

    fn supported_archs(&self) -> Vec<ModelArch> {
        vec![self.arch]
    }

    fn preferred_hardware(&self) -> &str {
        "gpu"
    }

    async fn load(&mut self) -> Result<(), InferenceError> {
        self.loaded = true;
        Ok(())
    }

    async fn unload(&mut self) -> Result<(), InferenceError> {
        self.loaded = false;
        Ok(())
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    async fn infer(&self, input: InferenceInput) -> Result<InferenceOutput, InferenceError> {
        match input {
            InferenceInput::Text { system, prompt, max_tokens, temperature } => {
                let req = CompletionRequest {
                    system,
                    prompt,
                    output_schema: None,
                    temperature,
                    max_tokens,
                    estimated_prompt_tokens: 0,
                };
                let resp = self.backend.complete(req).await
                    .map_err(|e| InferenceError::Engine(e.to_string()))?;
                Ok(InferenceOutput::Text {
                    text: resp.text,
                    usage: resp.usage,
                })
            }
            _ => Err(InferenceError::UnsupportedInput {
                engine: self.name.clone(),
                input_type: format!("{input:?}"),
            }),
        }
    }

    fn estimated_vram_mb(&self) -> u64 { self.vram_mb }
    fn estimated_ram_mb(&self) -> u64 { self.ram_mb }
}

// ---------------------------------------------------------------------------
// Inference Registry — manages all engines and routes requests
// ---------------------------------------------------------------------------

pub struct InferenceRegistry {
    /// Registered engines, keyed by model ID
    #[allow(dead_code)]
    engines: HashMap<String, Box<dyn InferenceEngine>>,
    /// Hardware capabilities (detected once at startup)
    pub hw: HardwareCapabilities,
    /// Performance profiles for known models
    pub profiles: Vec<PerformanceProfile>,
    /// Model download registry
    pub downloads: Vec<downloader::ModelEntry>,
}

impl InferenceRegistry {
    /// Create a new registry, auto-detecting hardware.
    pub async fn new(model_dir: PathBuf) -> Self {
        let hw = HardwareCapabilities::detect().await;
        let profiles = known_profiles(&hw);
        let downloads = downloader::suggested_downloads(model_dir);

        Self {
            engines: HashMap::new(),
            hw,
            profiles,
            downloads,
        }
    }

    /// Register an engine under a model ID.
    pub fn register(&mut self, model_id: impl Into<String>, engine: Box<dyn InferenceEngine>) {
        let id = model_id.into();
        info!(model_id = %id, engine = %engine.name(), "registered inference engine");
        self.engines.insert(id, engine);
    }

    /// Get an engine by model ID.
    pub fn get(&self, model_id: &str) -> Option<&dyn InferenceEngine> {
        self.engines.get(model_id).map(|e| e.as_ref())
    }

    /// Get a mutable engine by model ID.
    pub fn get_mut(&mut self, model_id: &str) -> Option<&mut Box<dyn InferenceEngine>> {
        self.engines.get_mut(model_id)
    }

    /// List all registered model IDs.
    pub fn list_models(&self) -> Vec<&str> {
        self.engines.keys().map(|s| s.as_str()).collect()
    }

    /// List registered models with their load status.
    pub fn model_status(&self) -> Vec<ModelStatus> {
        self.engines
            .iter()
            .map(|(id, engine)| ModelStatus {
                model_id: id.clone(),
                engine_name: engine.name().to_string(),
                loaded: engine.is_loaded(),
                vram_mb: engine.estimated_vram_mb(),
                ram_mb: engine.estimated_ram_mb(),
            })
            .collect()
    }

    /// Find the best engine for a given task.
    pub fn select(&self, task_type: ModelArch) -> Option<&dyn InferenceEngine> {
        // Prefer GPU engines, then CPU
        for engine in self.engines.values() {
            if engine.supported_archs().contains(&task_type) && engine.is_loaded() {
                return Some(engine.as_ref());
            }
        }
        // Fall back to any compatible engine
        for engine in self.engines.values() {
            if engine.supported_archs().contains(&task_type) {
                return Some(engine.as_ref());
            }
        }
        None
    }

    /// Run inference on the best engine for the given input.
    pub async fn infer(&self, input: InferenceInput) -> Result<InferenceOutput, InferenceError> {
        let task_type = match &input {
            InferenceInput::Text { .. } => ModelArch::FfnTransformer,
            InferenceInput::Audio { .. } => ModelArch::Speech,
            InferenceInput::TextToSpeech { .. } => ModelArch::Speech,
            InferenceInput::Image { .. } => ModelArch::VisionEncoder,
            InferenceInput::TextToImage { .. } => ModelArch::Diffusion,
            InferenceInput::Embed { .. } => ModelArch::Embedding,
            InferenceInput::Classify { .. } => ModelArch::FfnTransformer,
        };

        let engine = self.select(task_type).ok_or_else(|| {
            InferenceError::Engine(format!(
                "no engine available for task type {task_type:?}. Registered: {:?}",
                self.list_models()
            ))
        })?;

        engine.infer(input).await
    }

    /// Show a summary of capabilities.
    pub fn print_report(&self) {
        println!("\n═══════════════════════════════════════════════════");
        println!("  RoCo AI — Inference System Report");
        println!("═══════════════════════════════════════════════════");

        println!("\n  📦 Hardware:");
        println!("    CPU: {} ({} cores/{} threads)",
            self.hw.cpu.name, self.hw.cpu.cores, self.hw.cpu.threads);
        if let Some(gpu) = &self.hw.gpu {
            println!("    GPU: {} ({} MB VRAM, coop_matrix={})",
                gpu.name, gpu.vram_total_mb, gpu.has_cooperative_matrix);
        } else {
            println!("    GPU: none detected");
        }
        println!("    RAM: {} MB total, {} MB available",
            self.hw.total_ram_mb, self.hw.available_ram_mb);
        println!("    SSD: {} MB/s", self.hw.ssd_read_mb_s);

        println!("\n  📊 Performance Profiles:");
        for p in &self.profiles {
            println!("    {}", format_profile(p));
        }

        println!("\n  🧠 Models Available:");
        if self.engines.is_empty() {
            println!("    (none loaded — use roco-infer or Registry::register)");
        }
        for status in self.model_status() {
            let loaded = if status.loaded { "✅" } else { "⏳" };
            println!("    {loaded} {:<30} vram={}MB ram={}MB",
                status.model_id, status.vram_mb, status.ram_mb);
        }

        println!("\n  ⬇️  Suggested Downloads:");
        for entry in &self.downloads {
            let status = if entry.downloaded { "✅" } else { "⬇️ " };
            println!("    {status} {:<50} {:>6} MB  [{}]",
                entry.hf_id, entry.size_mb, entry.category);
        }
        println!("\n  Total download size: {} MB",
            self.downloads.iter().map(|e| e.size_mb).sum::<u64>());
        println!();
    }

    /// VRAM budget: how much is used vs available.
    pub fn vram_budget(&self) -> (u64, u64) {
        let used: u64 = self.engines
            .values()
            .filter(|e| e.is_loaded())
            .map(|e| e.estimated_vram_mb())
            .sum();
        let available = self.hw.gpu.as_ref().map(|g| g.vram_available_mb).unwrap_or(0);
        (used, available)
    }
}

/// Status of a registered model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatus {
    pub model_id: String,
    pub engine_name: String,
    pub loaded: bool,
    pub vram_mb: u64,
    pub ram_mb: u64,
}
