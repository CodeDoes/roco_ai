//! RWKVQuant-style proxy-guided quantization analysis.
//!
//! Based on "RWKVQuant: Quantizing the RWKV Family with Proxy Guided
//! Hybrid of Scalar and Vector Quantization" (ICML 2025, Houmo AI).
//!
//! For each weight tensor, computes:
//! - **P_c** (coarse proxy): Information entropy of sorted weight intervals.
//!   Low = uniform distribution (good for scalar quant like NF4/Int8).
//!   High = non-uniform (needs VQ, which we don't support yet).
//! - **P_f** (fine proxy): Taylor expansion of P_c → weighted sum of
//!   higher-order central moments (variance, skewness, kurtosis).
//!   Detects local outliers that P_c misses.
//!
//! Decision rule:
//! - P_c < τ_c AND P_f < τ_f → safe for scalar quant (NF4/Int8)
//! - Otherwise → skip quantization (keep FP16)
//!
//! ## Streaming analysis
//!
//! `analyze_model_streaming` reads the SafeTensors file tensor-by-tensor
//! via raw file I/O (header parse + `File::seek` + per-tensor `read`),
//! never loading the entire model into RAM. Peak memory is bounded to
//! the largest single tensor (~tens of MB), not the whole model (5+ GB).
//! Every tensor is analysed — no per-layer sampling.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

// ---------------------------------------------------------------------------
// Streaming SafeTensors reader
// ---------------------------------------------------------------------------

/// Minimal tensor metadata parsed from the SafeTensors header.
#[derive(Debug)]
struct StTensor {
    name: String,
    dtype: String,
    shape: Vec<usize>,
    data_offsets: (usize, usize), // [start, end) byte offsets
}

/// Parse the SafeTensors header from a file, returning tensor metadata.
///
/// SafeTensors format:
///   bytes 0-7:   header length as u64 LE
///   bytes 8..:   JSON header (tensor name → {dtype, shape, data_offsets})
///   after JSON:  raw tensor data
fn parse_st_header(file: &mut File) -> anyhow::Result<Vec<StTensor>> {
    // Read header length (u64 LE)
    let mut len_bytes = [0u8; 8];
    file.read_exact(&mut len_bytes)?;
    let header_len = u64::from_le_bytes(len_bytes) as usize;

    // Read header JSON
    let mut header_json = vec![0u8; header_len];
    file.read_exact(&mut header_json)?;

    // Parse JSON — the header is a map: name → {dtype, shape, data_offsets}
    let header: serde_json::Value = serde_json::from_slice(&header_json)?;

    let mut tensors = Vec::new();
    if let serde_json::Value::Object(map) = header {
        for (name, info) in map {
            let dtype = info.get("dtype").and_then(|v| v.as_str()).unwrap_or("F32").to_string();
            let shape = info.get("shape")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|u| u as usize)).collect())
                .unwrap_or_default();
            let offsets = info.get("data_offsets")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    let start = arr[0].as_u64().unwrap_or(0) as usize;
                    let end = arr[1].as_u64().unwrap_or(0) as usize;
                    (start, end)
                })
                .unwrap_or((0, 0));

            tensors.push(StTensor { name, dtype, shape, data_offsets: offsets });
        }
    }

    // Sort by data offset for sequential reading
    tensors.sort_by_key(|t| t.data_offsets.0);
    Ok(tensors)
}

/// Read a single tensor's raw bytes from a SafeTensors file.
fn read_tensor_bytes(file: &mut File, offsets: (usize, usize)) -> io::Result<Vec<u8>> {
    let len = offsets.1 - offsets.0;
    if len == 0 {
        return Ok(Vec::new());
    }
    file.seek(SeekFrom::Start(offsets.0 as u64))?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Decode raw tensor bytes to f32 values based on dtype string.
fn decode_tensor_bytes(dtype: &str, data: &[u8]) -> Vec<f32> {
    match dtype {
        "F32" => {
            let words = unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
            };
            words.to_vec()
        }
        "F16" => {
            let halves = unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const half::f16, data.len() / 2)
            };
            halves.iter().map(|&h| h.to_f32()).collect()
        }
        "BF16" => {
            let halves = unsafe {
                std::slice::from_raw_parts(data.as_ptr() as *const half::bf16, data.len() / 2)
            };
            halves.iter().map(|&h| h.to_f32()).collect()
        }
        _ => Vec::new(),
    }
}

/// Sample a weight vector to keep analysis fast. Takes evenly spaced
/// elements up to `max_samples`.
fn sample_weights(weights: &[f32], max_samples: usize) -> Vec<f32> {
    if weights.len() <= max_samples {
        return weights.to_vec();
    }
    let step = weights.len() / max_samples;
    (0..max_samples).map(|i| weights[i * step]).collect()
}

// ---------------------------------------------------------------------------
// Proxy computation
// ---------------------------------------------------------------------------

/// Compute the **coarse proxy** P_c for a weight vector.
///
/// Algorithm (from the paper, Section 4.1):
/// 1. Sort weights ascending → W'
/// 2. Compute adjacent intervals: G[i] = W'[i+1] - W'[i]
/// 3. Normalize: G'[i] = G[i] / ΣG
/// 4. Information entropy: H(G') = -Σ G'[i] · ln(G'[i])
/// 5. P_c = H(uniform) - H(G') = ln(n) - H(G')
///
/// Returns a value ≥ 0.  Lower = more uniform (better for scalar quant).
pub fn coarse_proxy(weights: &[f32]) -> f64 {
    if weights.len() < 2 {
        return 0.0;
    }

    // Step 1: sort ascending
    let mut sorted: Vec<f64> = weights.iter().map(|&w| w as f64).collect();
    sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Step 2: adjacent intervals
    let n = sorted.len() - 1;
    let intervals: Vec<f64> = sorted.windows(2).map(|w| w[1] - w[0]).collect();

    // Step 3: normalize to probability distribution
    let sum: f64 = intervals.iter().sum();
    if sum == 0.0 {
        return 0.0; // all weights identical — perfectly uniform
    }
    let g_prime: Vec<f64> = intervals.iter().map(|&g| g / sum).collect();

    // Step 4: information entropy H(G')
    let h: f64 = g_prime
        .iter()
        .filter(|&&g| g > 0.0)
        .map(|&g| -g * g.ln())
        .sum();

    // Step 5: P_c = H(uniform) - H(G') = ln(n) - H(G')
    let h_uniform = (n as f64).ln();
    h_uniform - h
}

/// Compute the **fine proxy** P_f for a weight vector.
///
/// Algorithm (from the paper, Section 4.1, Step 1-5):
/// 1. δ[i] = G'[i] - 1/n  (deviation from uniform)
/// 2. Taylor expansion of P_c around uniform distribution
/// 3. P_f = Σ_{k=2}^{K} v_k · |M_k|
///    where v_k = n^k / (k(k-1)), M_k = (1/n) Σ δ[i]^k
///
/// Uses K=4 (variance, skewness, kurtosis) as the paper recommends.
/// Higher P_f = more outliers present (VQ preferred).
pub fn fine_proxy(weights: &[f32]) -> f64 {
    if weights.len() < 2 {
        return 0.0;
    }

    // Steps 1-3: same sorting, intervals, normalization as coarse_proxy
    let mut sorted: Vec<f64> = weights.iter().map(|&w| w as f64).collect();
    sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = sorted.len() - 1;
    let intervals: Vec<f64> = sorted.windows(2).map(|w| w[1] - w[0]).collect();
    let sum: f64 = intervals.iter().sum();
    if sum == 0.0 {
        return 0.0;
    }
    let g_prime: Vec<f64> = intervals.iter().map(|&g| g / sum).collect();

    // Step 1 (fine): δ[i] = G'[i] - 1/n
    let inv_n = 1.0 / n as f64;
    let delta: Vec<f64> = g_prime.iter().map(|&g| g - inv_n).collect();

    // Steps 2-4: P_f = Σ_{k=2}^{K} v_k · |M_k|
    // v_k = n^k / (k(k-1)),  M_k = (1/n) Σ δ[i]^k
    let n_f = n as f64;
    let mut p_f = 0.0f64;
    for k in 2..=4 {
        let v_k = n_f.powi(k) / (k * (k - 1)) as f64;
        let m_k: f64 = delta.iter().map(|&d| d.powi(k)).sum::<f64>() / n_f;
        p_f += v_k * m_k.abs();
    }
    p_f
}

// ---------------------------------------------------------------------------
// Thresholds and quantization decisions
// ---------------------------------------------------------------------------

/// Default coarse proxy threshold.  Tensors with P_c < this are considered
/// "uniform enough" for scalar quantization.  RWKV weights are inherently
/// non-uniform (rwkv.cpp#12), so this is set higher than the paper's 0.05.
pub const DEFAULT_TAU_C: f64 = 4.0;

/// Default fine proxy threshold.  RWKV P_f values are enormous (n^k
/// amplification) so we use a percentile-based default instead.
/// Will be overridden by `DEFAULT_TAU_F_PERCENTILE` if analysis provides data.
pub const DEFAULT_TAU_F: f64 = 1e6;

/// Use the Nth percentile of P_f values as the threshold, overriding
/// `DEFAULT_TAU_F` when we have enough data.  This adapts to the model.
const DEFAULT_TAU_F_PERCENTILE: f64 = 0.25; // 25th percentile

/// Per-tensor quantization recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantRecommendation {
    /// Uniform, no outliers → safe for NF4 or Int8.
    ScalarQuant,
    /// Non-uniform or has outliers → keep FP16 (we don't support VQ).
    KeepFp16,
}

/// Decide quantization method for a tensor given its proxy values.
///
/// Rule from the paper: only use scalar quant if BOTH proxies are below
/// their thresholds.  Otherwise, the tensor needs VQ (which we skip).
pub fn recommend(
    p_c: f64,
    p_f: f64,
    tau_c: f64,
    tau_f: f64,
) -> QuantRecommendation {
    if p_c < tau_c && p_f < tau_f {
        QuantRecommendation::ScalarQuant
    } else {
        QuantRecommendation::KeepFp16
    }
}

// ---------------------------------------------------------------------------
// Model-level analysis
// ---------------------------------------------------------------------------

/// Analysis result for a single tensor.
#[derive(Debug)]
pub struct TensorAnalysis {
    pub name: String,
    pub numels: usize,
    pub p_c: f64,
    pub p_f: f64,
    pub recommendation: QuantRecommendation,
}

/// Analysis result for an entire model.
#[derive(Debug)]
pub struct ModelAnalysis {
    pub tensors: Vec<TensorAnalysis>,
    pub total_tensors: usize,
    pub sq_count: usize,
    pub fp16_count: usize,
    pub sq_numels: usize,
    pub fp16_numels: usize,
}

impl ModelAnalysis {
    /// Pretty-print the analysis to stdout.
    pub fn print(&self) {
        println!("\n{:─<80}", "");
        println!("  RWKVQuant Proxy Analysis");
        println!("  τ_c = {:<10}  τ_f = {}", DEFAULT_TAU_C, DEFAULT_TAU_F);
        println!("{:─<80}", "");
        println!(
            "{:<45} {:>8} {:>10} {:>10}  {:>6}",
            "tensor", "numels", "P_c", "P_f", "rec"
        );
        println!("{:-<80}", "");

        for t in &self.tensors {
            let rec_str = match t.recommendation {
                QuantRecommendation::ScalarQuant => "SQ  ",
                QuantRecommendation::KeepFp16 => "FP16",
            };
            println!(
                "{:<45} {:>8} {:>10.4} {:>10.4}  {:>6}",
                t.name, t.numels, t.p_c, t.p_f, rec_str
            );
        }

        println!("{:-<80}", "");
        println!(
            "  Tensors:  SQ={}  FP16={}  total={}",
            self.sq_count, self.fp16_count, self.total_tensors
        );
        println!(
            "  Elements: SQ={}M ({:.1}%)  FP16={}M ({:.1}%)",
            self.sq_numels / 1_000_000,
            100.0 * self.sq_numels as f64 / (self.sq_numels + self.fp16_numels) as f64,
            self.fp16_numels / 1_000_000,
            100.0 * self.fp16_numels as f64 / (self.sq_numels + self.fp16_numels) as f64,
        );
        println!("{:─<80}\n", "");
    }
}

/// Analyze a SafeTensors model file, computing P_c and P_f for every
/// weight tensor. Uses streaming I/O — never loads the entire model into
/// RAM. Peak memory is bounded to the largest single tensor.
pub fn analyze_model_streaming(path: &str) -> anyhow::Result<ModelAnalysis> {
    let mut file = File::open(path)?;
    let tensors_meta = parse_st_header(&mut file)?;

    let mut tensors = Vec::new();
    let total = tensors_meta.len();

    for (i, t) in tensors_meta.iter().enumerate() {
        let numel: usize = t.shape.iter().product();
        if numel < 2 {
            continue;
        }
        // Only analyse float tensors
        if !matches!(t.dtype.as_str(), "F32" | "F16" | "BF16") {
            continue;
        }

        // Read this one tensor's bytes, decode, compute proxies, discard
        let raw = read_tensor_bytes(&mut file, t.data_offsets)?;
        if raw.is_empty() {
            continue;
        }
        let weights = decode_tensor_bytes(&t.dtype, &raw);
        if weights.len() < 2 {
            continue;
        }

        // Sample if very large to keep analysis fast
        let sample = sample_weights(&weights, 65_536);
        let p_c = coarse_proxy(&sample);
        let p_f = fine_proxy(&sample);
        let rec = recommend(p_c, p_f, DEFAULT_TAU_C, DEFAULT_TAU_F);

        // Print progress every 50 tensors
        if (i + 1) % 50 == 0 || i + 1 == total {
            eprintln!("  [{:>4}/{:>4}] {}  P_c={:.4}  P_f={:.4}",
                i + 1, total, t.name, p_c, p_f);
        }

        tensors.push(TensorAnalysis {
            name: t.name.clone(),
            numels: numel,
            p_c,
            p_f,
            recommendation: rec,
        });
    }

    tensors.sort_by(|a, b| a.name.cmp(&b.name));

    // Compute adaptive τ_f from the P_f distribution if we have enough data.
    // This avoids the fixed-threshold problem where RWKV P_f values span
    // orders of magnitude (3 500 → 3 trillion).
    let tau_f = if tensors.len() >= 10 {
        let mut pfs: Vec<f64> = tensors.iter().map(|t| t.p_f).collect();
        // Handle NaN: treat as +infinity so it sorts to the end
        pfs.sort_by(|a, b| {
            match (a.is_nan(), b.is_nan()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                (false, false) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
            }
        });
        let idx = (DEFAULT_TAU_F_PERCENTILE * pfs.len() as f64) as usize;
        let adaptive = pfs[idx.min(pfs.len() - 1)];
        eprintln!("  Adaptive τ_f = {:.4} ({}th pctile of {} tensors)",
            adaptive, DEFAULT_TAU_F_PERCENTILE * 100.0, pfs.len());
        adaptive
    } else {
        DEFAULT_TAU_F
    };

    // Recompute recommendations with adaptive τ_f
    let mut sq_count = 0;
    let mut fp16_count = 0;
    let mut sq_numels = 0;
    let mut fp16_numels = 0;
    for t in &mut tensors {
        t.recommendation = recommend(t.p_c, t.p_f, DEFAULT_TAU_C, tau_f);
        match t.recommendation {
            QuantRecommendation::ScalarQuant => { sq_count += 1; sq_numels += t.numels; }
            QuantRecommendation::KeepFp16 => { fp16_count += 1; fp16_numels += t.numels; }
        }
    }

    Ok(ModelAnalysis {
        tensors,
        total_tensors: sq_count + fp16_count,
        sq_count,
        fp16_count,
        sq_numels,
        fp16_numels,
    })
}

/// Backwards-compatible wrapper: analyse from an in-memory slice.
/// Convenient for tests; avoid in production for large models.
pub fn analyze_model(st_data: &[u8]) -> anyhow::Result<ModelAnalysis> {
    let st = safetensors::SafeTensors::deserialize(st_data)?;

    let mut tensors = Vec::new();
    let mut sq_count = 0;
    let mut fp16_count = 0;
    let mut sq_numels = 0;
    let mut fp16_numels = 0;

    for (name, view) in st.tensors() {
        let numel = view.shape().iter().product::<usize>();
        if numel < 2 {
            continue;
        }
        let weights: Vec<f32> = match view.dtype() {
            safetensors::Dtype::F32 => {
                let bytes = view.data();
                unsafe {
                    std::slice::from_raw_parts(bytes.as_ptr() as *const f32, bytes.len() / 4)
                }.to_vec()
            }
            safetensors::Dtype::F16 => {
                let bytes = view.data();
                unsafe {
                    std::slice::from_raw_parts(bytes.as_ptr() as *const half::f16, bytes.len() / 2)
                }.iter().map(|&h| h.to_f32()).collect()
            }
            safetensors::Dtype::BF16 => {
                let bytes = view.data();
                unsafe {
                    std::slice::from_raw_parts(bytes.as_ptr() as *const half::bf16, bytes.len() / 2)
                }.iter().map(|&h| h.to_f32()).collect()
            }
            _ => continue,
        };
        let p_c = coarse_proxy(&weights);
        let p_f = fine_proxy(&weights);
        let rec = recommend(p_c, p_f, DEFAULT_TAU_C, DEFAULT_TAU_F);
        match rec {
            QuantRecommendation::ScalarQuant => { sq_count += 1; sq_numels += numel; }
            QuantRecommendation::KeepFp16 => { fp16_count += 1; fp16_numels += numel; }
        }
        tensors.push(TensorAnalysis { name, numels: numel, p_c, p_f, recommendation: rec });
    }
    tensors.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ModelAnalysis {
        tensors,
        total_tensors: sq_count + fp16_count,
        sq_count,
        fp16_count,
        sq_numels,
        fp16_numels,
    })
}

/// Build a per-layer quantization plan from a [`ModelAnalysis`].
///
/// For layers where ALL tensors recommend ScalarQuant, we use NF4
/// (if GPU supports coop matrix) or Int8 (fallback).  For layers
/// with ANY tensor recommending KeepFp16, we skip quantization.
///
/// Returns a HashMap mapping layer index → Quant type, compatible
/// with `web_rwkv::runtime::model::ModelBuilder::quant()`.
pub fn build_quant_plan(
    analysis: &ModelAnalysis,
    gpu_coop: bool,
    num_layers: usize,
) -> HashMap<usize, web_rwkv::runtime::model::Quant> {
    use web_rwkv::runtime::model::Quant;

    // Group tensors by layer
    let mut layer_has_fp16: Vec<bool> = vec![false; num_layers];

    for t in &analysis.tensors {
        // Extract layer index from tensor name
        // RWKV convention: "blk.{N}.{weight_name}"
        if let Some(layer) = extract_layer(&t.name) {
            if t.recommendation == QuantRecommendation::KeepFp16 {
                layer_has_fp16[layer] = true;
            }
        }
    }

    // Build quant plan
    let mut plan = HashMap::new();
    let q = if gpu_coop { Quant::NF4 } else { Quant::Int8 };

    for layer in 0..num_layers {
        if !layer_has_fp16[layer] {
            plan.insert(layer, q);
        }
        // Layers with FP16 tensors are omitted → no quantization
    }

    plan
}

/// Extract the layer index from a tensor name.
/// RWKV SafeTensors use `blk.{N}.{weight_name}` convention.
fn extract_layer(name: &str) -> Option<usize> {
    // Pattern: "blk.N..." or "blocks.N..."
    let parts: Vec<&str> = name.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if (*part == "blk" || *part == "blocks") && i + 1 < parts.len() {
            return parts[i + 1].parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_weights_have_low_proxy() {
        // Perfectly uniform weights: intervals are all equal
        let weights: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let p_c = coarse_proxy(&weights);
        let p_f = fine_proxy(&weights);
        assert!(p_c < 0.001, "P_c for uniform weights should be near 0, got {p_c}");
        assert!(p_f < 0.001, "P_f for uniform weights should be near 0, got {p_f}");
    }

    #[test]
    fn weights_with_outliers_have_high_fine_proxy() {
        // Mostly uniform with one big outlier
        let mut weights: Vec<f32> = (0..100).map(|i| i as f32).collect();
        weights.push(10000.0); // outlier
        let p_c = coarse_proxy(&weights);
        let p_f = fine_proxy(&weights);
        assert!(
            p_f > p_c,
            "P_f should be higher than P_c when outliers are present (P_f={p_f}, P_c={p_c})"
        );
    }

    #[test]
    fn single_element_returns_zero() {
        let weights = vec![1.0f32];
        assert_eq!(coarse_proxy(&weights), 0.0);
        assert_eq!(fine_proxy(&weights), 0.0);
    }

    #[test]
    fn identical_weights_return_zero() {
        let weights = vec![5.0f32; 100];
        assert_eq!(coarse_proxy(&weights), 0.0);
        assert_eq!(fine_proxy(&weights), 0.0);
    }

    #[test]
    fn recommend_uniform_gets_sq() {
        // Low P_c + low P_f → safe for scalar quant
        assert_eq!(
            recommend(0.01, 0.001, DEFAULT_TAU_C, DEFAULT_TAU_F),
            QuantRecommendation::ScalarQuant
        );
    }

    #[test]
    fn recommend_non_uniform_gets_fp16() {
        // High P_c (non-uniform distribution) → needs FP16
        // Use tight thresholds to test the logic
        assert_eq!(
            recommend(0.1, 0.001, 0.05, 0.01),
            QuantRecommendation::KeepFp16
        );
    }

    #[test]
    fn recommend_outliers_gets_fp16() {
        // Low P_c but high P_f (uniform with outliers) → needs FP16
        assert_eq!(
            recommend(0.01, 0.1, 0.05, 0.01),
            QuantRecommendation::KeepFp16
        );
    }

    #[test]
    fn extract_layer_from_blk_name() {
        assert_eq!(extract_layer("blk.5.time_mix.weight"), Some(5));
        assert_eq!(extract_layer("blk.0.channel_mix.weight"), Some(0));
        assert_eq!(extract_layer("emb.weight"), None);
    }
}
