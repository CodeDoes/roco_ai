//! Theoretical performance model for local inference.
//!
//! Estimates load time, inference speed, and memory usage based on model
//! architecture, size, quantization, and hardware capabilities. These are
//! **theoretical estimates** from literature and architecture analysis — they
//! get refined by actual measurement.
//!
//! ## Key Factors
//!
//! | Factor | Impact | Why |
//! |---|---|---|
//! | Parameter count | O(n) memory, O(n²) compute | Attention is quadratic |
//! | Quantization | 2-4x memory savings | Int8 halves, NF4 quarters |
//! | Architecture | RNN vs FFN vs Diffusion | RNN is linear-time, FFN is quadratic |
//! | GPU VRAM | Max model size | 4GB fits ~3B Int8, not 7B+ |
//! | PCIe bandwidth | Upload speed | 5-6 GB/s practical on RTX 2050 |
//! | Shader compilation | First-load latency | 5-20s for WGPU on first run |
//! | NVMe vs SATA | Model read speed | 3.5 GB/s vs 500 MB/s |

use serde::{Deserialize, Serialize};

use super::hardware::HardwareCapabilities;

/// Model architecture family — determines compute characteristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelArch {
    /// RWKV-style linear RNN (linear time, stateful)
    RnnRwkv,
    /// Standard transformer with attention (quadratic in context)
    FfnTransformer,
    /// Mixture-of-Experts transformer (sparse FFN)
    MoE,
    /// Diffusion model (iterative denoising)
    Diffusion,
    /// Vision encoder (ViT-based)
    VisionEncoder,
    /// Speech encoder/decoder
    Speech,
    /// Embedding model (dense encoder)
    Embedding,
}

/// Theoretical performance profile for a model on specific hardware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceProfile {
    /// Model name / identifier
    pub model: String,
    pub arch: ModelArch,
    pub params_b: f64,
    pub quant_bits: u32,

    // Memory
    pub vram_mb: u64,
    pub ram_mb: u64,
    pub resident_mb: u64,

    // Load time
    pub disk_read_ms: u64,
    pub deserialize_ms: u64,
    pub shader_compile_ms: u64,
    pub gpu_upload_ms: u64,
    pub total_load_ms: u64,
    pub subsequent_load_ms: u64,

    // Inference speed
    pub inference_tok_s: f64,
    pub prompt_processing_tok_s: f64,
    pub time_to_first_token_ms: u64,

    // Confidence: how much we trust these estimates
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Confidence {
    /// Measured directly — this is real data
    Measured,
    /// Estimated from similar model on same hardware
    Estimated,
    /// Theoretical guess based on literature
    Theoretical,
}

impl PerformanceProfile {
    /// Build a theoretical estimate for a model on given hardware.
    ///
    /// Based on:
    /// - Literature benchmarks (llama.cpp, candle, web-rwkv)
    /// - Architecture analysis (attention = O(n²), RNN = O(n))
    /// - Hardware specs (VRAM bandwidth, PCIe gen, RAM speed)
    /// - Actual measured data from our RWKV 2.9B baseline
    pub fn estimate(
        model: &str,
        arch: ModelArch,
        params_b: f64,
        quant_bits: u32,
        hw: &HardwareCapabilities,
    ) -> Self {
        // --- Memory --- //
        let bytes_per_param = quant_bits as f64 / 8.0;
        let model_mb = (params_b * 1_000_000_000.0 * bytes_per_param) / (1024.0 * 1024.0);
        let overhead = match arch {
            ModelArch::RnnRwkv => 0.30,    // 30% for state + KV cache
            ModelArch::FfnTransformer => 0.50, // 50% for KV cache + attention
            ModelArch::Diffusion => 1.0,   // 100% for intermediate latents
            _ => 0.20,
        };
        let resident_mb = (model_mb * (1.0 + overhead)).ceil() as u64;
        // RAM is model + resident for CPU fallback
        let ram_mb = (model_mb * 1.5).ceil() as u64;
        let vram_mb = resident_mb;

        // --- Load time --- //
        let disk_read_ms = ((model_mb as f64) / hw.ssd_read_mb_s as f64 * 1000.0).ceil() as u64;

        let deserialize_ms = match arch {
            ModelArch::RnnRwkv => {
                // SafeTensors deserialization: ~2s for 5.5GB
                (2000.0 * (params_b / 2.9)).ceil() as u64
            }
            ModelArch::FfnTransformer => {
                // GGUF is memory-mapped, ~100ms; safetensors needs ~500ms
                (500.0 * (params_b / 2.0)).ceil() as u64
            }
            _ => 500,
        };

        let shader_compile_ms = match &hw.gpu {
            Some(gpu) if gpu.device_type == "DiscreteGpu" => {
                // NVIDIA WGPU shader compilation: ~15s first time
                match arch {
                    ModelArch::RnnRwkv => 14000,
                    ModelArch::FfnTransformer => 8000,
                    _ => 3000,
                }
            }
            // Integrated GPU / CPU: faster compilation
            _ => 2000,
        };

        let gpu_upload_ms = if hw.gpu.is_some() && hw.fits_in_vram(vram_mb) {
            // PCIe upload: model_size / bandwidth
            ((model_mb as f64) / hw.pcie_bandwidth_mb_s as f64 * 1000.0).ceil() as u64
        } else {
            0 // CPU inference, no upload
        };

        let total_load_ms = disk_read_ms + deserialize_ms + shader_compile_ms + gpu_upload_ms;
        let subsequent_load_ms = if hw.gpu.is_some() && hw.fits_in_vram(vram_mb) {
            gpu_upload_ms // only need to upload again
        } else {
            disk_read_ms + deserialize_ms
        };

        // --- Inference speed --- //
        // Base rate: tok/s per billion params on this hardware
        let gpu_base = match &hw.gpu {
            Some(gpu) if gpu.device_type == "DiscreteGpu" => {
                match arch {
                    ModelArch::RnnRwkv => 40.0,  // ~16-26 measured for 2.9B → ~40/paramB
                    ModelArch::FfnTransformer => 30.0,
                    ModelArch::MoE => 50.0,  // Sparse: fewer active params
                    _ => 20.0,
                }
            }
            Some(gpu) if gpu.device_type == "IntegratedGpu" => {
                match arch {
                    ModelArch::RnnRwkv => 15.0,
                    ModelArch::FfnTransformer => 10.0,
                    _ => 5.0,
                }
            }
            _ => 0.0, // CPU handled below
        };

        let cpu_base = match arch {
            ModelArch::RnnRwkv => 5.0 * (4000.0 / hw.cpu.threads as f64).max(1.0),
            ModelArch::FfnTransformer => 3.0 * (4000.0 / hw.cpu.threads as f64).max(1.0),
            ModelArch::MoE => 5.0,
            _ => 2.0,
        };

        let (inference_tok_s, prompt_processing_tok_s, time_to_first_token_ms) =
            if hw.gpu.is_some() && hw.fits_in_vram(vram_mb) {
                let tok_s = (gpu_base / params_b).max(1.0);
                // Prompt processing is faster: 2-3x generation speed
                let pp_tok_s = tok_s * 2.5;
                let ttft = (500.0 * params_b).max(50.0) as u64;
                (tok_s, pp_tok_s, ttft)
            } else {
                let tok_s = (cpu_base / params_b).max(0.5);
                let pp_tok_s = tok_s * 1.2;
                let ttft = (2000.0 * params_b).max(100.0) as u64;
                (tok_s, pp_tok_s, ttft)
            };

        let confidence = Confidence::Estimated;

        Self {
            model: model.to_string(),
            arch,
            params_b,
            quant_bits,
            vram_mb,
            ram_mb,
            resident_mb,
            disk_read_ms,
            deserialize_ms,
            shader_compile_ms,
            gpu_upload_ms,
            total_load_ms,
            subsequent_load_ms,
            inference_tok_s,
            prompt_processing_tok_s,
            time_to_first_token_ms,
            confidence,
        }
    }

    /// Create a profile from actual measured data (replaces estimates).
    pub fn measured(
        model: &str,
        arch: ModelArch,
        params_b: f64,
        quant_bits: u32,
        total_load_ms: u64,
        inference_tok_s: f64,
        vram_mb: u64,
    ) -> Self {
        Self {
            model: model.to_string(),
            arch,
            params_b,
            quant_bits,
            vram_mb,
            ram_mb: vram_mb * 2,
            resident_mb: vram_mb,
            disk_read_ms: total_load_ms / 4,
            deserialize_ms: total_load_ms / 4,
            shader_compile_ms: total_load_ms / 3,
            gpu_upload_ms: total_load_ms / 6,
            total_load_ms,
            subsequent_load_ms: total_load_ms / 4, // only upload
            inference_tok_s,
            prompt_processing_tok_s: inference_tok_s * 2.5,
            time_to_first_token_ms: (1000.0 / inference_tok_s).ceil() as u64,
            confidence: Confidence::Measured,
        }
    }
}

// ---------------------------------------------------------------------------
// Known model profiles (pre-computed estimates, updated with real data)
// ---------------------------------------------------------------------------

pub fn known_profiles(hw: &HardwareCapabilities) -> Vec<PerformanceProfile> {
    vec![
        // Already measured:
        PerformanceProfile::measured(
            "rwkv7-g1g-2.9b (Int8)",
            ModelArch::RnnRwkv,
            2.9, 8,
            18000, // actual load time from testing
            20.0,  // actual tok/s measured
            2750,  // actual VRAM
        ),
        // Theoretical:
        PerformanceProfile::estimate(
            "qwen2.5-coder-1.5b (FP16)",
            ModelArch::FfnTransformer,
            1.5, 16,
            hw,
        ),
        PerformanceProfile::estimate(
            "tinyllama-1.1b (Int8)",
            ModelArch::FfnTransformer,
            1.1, 8,
            hw,
        ),
        PerformanceProfile::estimate(
            "minicpm5-1b (FP16)",
            ModelArch::FfnTransformer,
            1.0, 16,
            hw,
        ),
        PerformanceProfile::estimate(
            "smollm2-360m (FP16)",
            ModelArch::FfnTransformer,
            0.36, 16,
            hw,
        ),
        PerformanceProfile::estimate(
            "phi-4-mini (FP16)",
            ModelArch::FfnTransformer,
            3.8, 16,
            hw,
        ),
    ]
}

/// Format a profile for human-readable display.
pub fn format_profile(p: &PerformanceProfile) -> String {
    let conf = match p.confidence {
        Confidence::Measured => "📊",
        Confidence::Estimated => "📐",
        Confidence::Theoretical => "🔮",
    };
    let arch = match p.arch {
        ModelArch::RnnRwkv => "RNN",
        ModelArch::FfnTransformer => "FFN",
        ModelArch::MoE => "MoE",
        ModelArch::Diffusion => "DIFF",
        ModelArch::VisionEncoder => "VIS",
        ModelArch::Speech => "SPEECH",
        ModelArch::Embedding => "EMBED",
    };
    let model = &p.model;
    format!(
        "{conf} {model:40} | {arch:6} | {q}bit | VRAM={v}MB | LOAD={load}ms | {tok_s:>5.1} tok/s",
        conf = conf,
        model = model,
        arch = arch,
        q = p.quant_bits,
        v = p.vram_mb,
        load = p.total_load_ms,
        tok_s = p.inference_tok_s,
    )
}
