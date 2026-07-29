//! RWKVQuant-style proxy-guided quantization analysis.
//!
//! Based on "RWKVQuant: Quantizing the RWKV Family with Proxy Guided
//! Hybrid of Scalar and Vector Quantization" (ICML 2025, Houmo AI).

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

/// Minimal tensor metadata parsed from the SafeTensors header.
#[derive(Debug)]
struct StTensor {
    name: String,
    dtype: String,
    shape: Vec<usize>,
    data_offsets: (usize, usize),
}

fn parse_st_header(file: &mut File) -> anyhow::Result<Vec<StTensor>> {
    let mut len_bytes = [0u8; 8];
    file.read_exact(&mut len_bytes)?;
    let header_len = u64::from_le_bytes(len_bytes) as usize;
    let mut header_json = vec![0u8; header_len];
    file.read_exact(&mut header_json)?;
    let header: serde_json::Value = serde_json::from_slice(&header_json)?;

    let mut tensors = Vec::new();
    if let serde_json::Value::Object(map) = header {
        for (name, info) in map {
            let dtype = info
                .get("dtype")
                .and_then(|v| v.as_str())
                .unwrap_or("F32")
                .to_string();
            let shape = info
                .get("shape")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_u64().map(|u| u as usize))
                        .collect()
                })
                .unwrap_or_default();
            let offsets = info
                .get("data_offsets")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    let start = arr[0].as_u64().unwrap_or(0) as usize;
                    let end = arr[1].as_u64().unwrap_or(0) as usize;
                    (start, end)
                })
                .unwrap_or((0, 0));
            tensors.push(StTensor {
                name,
                dtype,
                shape,
                data_offsets: offsets,
            });
        }
    }
    tensors.sort_by_key(|t| t.data_offsets.0);
    Ok(tensors)
}

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

fn decode_tensor_bytes(dtype: &str, data: &[u8]) -> Vec<f32> {
    match dtype {
        "F32" => {
            let words =
                unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4) };
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

pub fn coarse_proxy(weights: &[f32]) -> f64 {
    if weights.len() < 2 {
        return 0.0;
    }
    let mut sorted: Vec<f64> = weights.iter().map(|&w| w as f64).collect();
    sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len() - 1;
    let intervals: Vec<f64> = sorted.windows(2).map(|w| w[1] - w[0]).collect();
    let sum: f64 = intervals.iter().sum();
    if sum == 0.0 {
        return 0.0;
    }
    let g_prime: Vec<f64> = intervals.iter().map(|&g| g / sum).collect();
    let h: f64 = g_prime
        .iter()
        .filter(|&&g| g > 0.0)
        .map(|&g| -g * g.ln())
        .sum();
    let h_uniform = (n as f64).ln();
    h_uniform - h
}

pub fn fine_proxy(weights: &[f32]) -> f64 {
    if weights.len() < 2 {
        return 0.0;
    }
    let mut sorted: Vec<f64> = weights.iter().map(|&w| w as f64).collect();
    sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len() - 1;
    let intervals: Vec<f64> = sorted.windows(2).map(|w| w[1] - w[0]).collect();
    let sum: f64 = intervals.iter().sum();
    if sum == 0.0 {
        return 0.0;
    }
    let g_prime: Vec<f64> = intervals.iter().map(|&g| g / sum).collect();
    let inv_n = 1.0 / n as f64;
    let delta: Vec<f64> = g_prime.iter().map(|&g| g - inv_n).collect();
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

pub const DEFAULT_TAU_C: f64 = 4.0;
pub const DEFAULT_TAU_F: f64 = 1e6;
const DEFAULT_TAU_F_PERCENTILE: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantRecommendation {
    ScalarQuant,
    KeepFp16,
}

pub fn recommend(p_c: f64, p_f: f64, tau_c: f64, tau_f: f64) -> QuantRecommendation {
    if p_c < tau_c && p_f < tau_f {
        QuantRecommendation::ScalarQuant
    } else {
        QuantRecommendation::KeepFp16
    }
}

// ---------------------------------------------------------------------------
// Model-level analysis
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct TensorAnalysis {
    pub name: String,
    pub numels: usize,
    pub p_c: f64,
    pub p_f: f64,
    pub recommendation: QuantRecommendation,
}

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

pub fn analyze_model_streaming(path: &str) -> anyhow::Result<ModelAnalysis> {
    let mut file = File::open(path)?;
    let tensors_meta = parse_st_header(&mut file)?;
    let mut tensors = Vec::new();
    let total = tensors_meta.len();

    for (i, t) in tensors_meta.iter().enumerate() {
        let numel: usize = t.shape.iter().product();
        if numel < 2 || !matches!(t.dtype.as_str(), "F32" | "F16" | "BF16") {
            continue;
        }

        let raw = read_tensor_bytes(&mut file, t.data_offsets)?;
        if raw.is_empty() {
            continue;
        }
        let weights = decode_tensor_bytes(&t.dtype, &raw);
        if weights.len() < 2 {
            continue;
        }

        let sample = sample_weights(&weights, 65_536);
        let p_c = coarse_proxy(&sample);
        let p_f = fine_proxy(&sample);
        let rec = recommend(p_c, p_f, DEFAULT_TAU_C, DEFAULT_TAU_F);

        if (i + 1) % 50 == 0 || i + 1 == total {
            eprintln!(
                "  [{:>4}/{:>4}] {}  P_c={:.4}  P_f={:.4}",
                i + 1,
                total,
                t.name,
                p_c,
                p_f
            );
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

    let tau_f = if tensors.len() >= 10 {
        let mut pfs: Vec<f64> = tensors.iter().map(|t| t.p_f).collect();
        pfs.sort_by(|a, b| match (a.is_nan(), b.is_nan()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
        });
        let idx = (DEFAULT_TAU_F_PERCENTILE * pfs.len() as f64) as usize;
        let adaptive = pfs[idx.min(pfs.len() - 1)];
        eprintln!(
            "  Adaptive τ_f = {:.4} ({}th pctile of {} tensors)",
            adaptive,
            DEFAULT_TAU_F_PERCENTILE * 100.0,
            pfs.len()
        );
        adaptive
    } else {
        DEFAULT_TAU_F
    };

    let mut sq_count = 0;
    let mut fp16_count = 0;
    let mut sq_numels = 0;
    let mut fp16_numels = 0;
    for t in &mut tensors {
        t.recommendation = recommend(t.p_c, t.p_f, DEFAULT_TAU_C, tau_f);
        match t.recommendation {
            QuantRecommendation::ScalarQuant => {
                sq_count += 1;
                sq_numels += t.numels;
            }
            QuantRecommendation::KeepFp16 => {
                fp16_count += 1;
                fp16_numels += t.numels;
            }
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
            safetensors::Dtype::F32 => unsafe {
                std::slice::from_raw_parts(
                    view.data().as_ptr() as *const f32,
                    view.data().len() / 4,
                )
            }
            .to_vec(),
            safetensors::Dtype::F16 => unsafe {
                std::slice::from_raw_parts(
                    view.data().as_ptr() as *const half::f16,
                    view.data().len() / 2,
                )
            }
            .iter()
            .map(|&h| h.to_f32())
            .collect(),
            safetensors::Dtype::BF16 => unsafe {
                std::slice::from_raw_parts(
                    view.data().as_ptr() as *const half::bf16,
                    view.data().len() / 2,
                )
            }
            .iter()
            .map(|&h| h.to_f32())
            .collect(),
            _ => continue,
        };
        let p_c = coarse_proxy(&weights);
        let p_f = fine_proxy(&weights);
        let rec = recommend(p_c, p_f, DEFAULT_TAU_C, DEFAULT_TAU_F);
        match rec {
            QuantRecommendation::ScalarQuant => {
                sq_count += 1;
                sq_numels += numel;
            }
            QuantRecommendation::KeepFp16 => {
                fp16_count += 1;
                fp16_numels += numel;
            }
        }
        tensors.push(TensorAnalysis {
            name: name.to_string(),
            numels: numel,
            p_c,
            p_f,
            recommendation: rec,
        });
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

pub fn build_quant_plan(
    analysis: &ModelAnalysis,
    gpu_coop: bool,
    num_layers: usize,
) -> HashMap<usize, web_rwkv::runtime::model::Quant> {
    use web_rwkv::runtime::model::Quant;
    let mut layer_has_fp16: Vec<bool> = vec![false; num_layers];
    for t in &analysis.tensors {
        if let Some(layer) = extract_layer(&t.name) {
            if t.recommendation == QuantRecommendation::KeepFp16 {
                layer_has_fp16[layer] = true;
            }
        }
    }
    let mut plan = HashMap::new();
    let q = if gpu_coop { Quant::NF4 } else { Quant::Int8 };
    for (layer, has_fp16) in layer_has_fp16.iter().enumerate() {
        if !has_fp16 {
            plan.insert(layer, q);
        }
    }
    plan
}

fn extract_layer(name: &str) -> Option<usize> {
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
        let weights: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let p_c = coarse_proxy(&weights);
        let p_f = fine_proxy(&weights);
        assert!(p_c < 0.001, "P_c={p_c}");
        assert!(p_f < 0.001, "P_f={p_f}");
    }

    #[test]
    fn weights_with_outliers_have_high_fine_proxy() {
        let mut weights: Vec<f32> = (0..100).map(|i| i as f32).collect();
        weights.push(10000.0);
        let p_c = coarse_proxy(&weights);
        let p_f = fine_proxy(&weights);
        assert!(p_f > p_c, "P_f={p_f} should be > P_c={p_c}");
    }

    #[test]
    fn single_element_returns_zero() {
        assert_eq!(coarse_proxy(&[1.0f32]), 0.0);
        assert_eq!(fine_proxy(&[1.0f32]), 0.0);
    }

    #[test]
    fn identical_weights_return_zero() {
        let weights = vec![5.0f32; 100];
        assert_eq!(coarse_proxy(&weights), 0.0);
        assert_eq!(fine_proxy(&weights), 0.0);
    }

    #[test]
    fn recommend_uniform_gets_sq() {
        assert_eq!(
            recommend(0.01, 0.001, DEFAULT_TAU_C, DEFAULT_TAU_F),
            QuantRecommendation::ScalarQuant
        );
    }

    #[test]
    fn recommend_non_uniform_gets_fp16() {
        assert_eq!(
            recommend(0.1, 0.001, 0.05, 0.01),
            QuantRecommendation::KeepFp16
        );
    }

    #[test]
    fn recommend_outliers_gets_fp16() {
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
