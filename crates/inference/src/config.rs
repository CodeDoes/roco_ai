//! Model configuration, quantization planning, and path resolution.
//!
//! Provides automatic quantization selection based on model size and GPU
//! capabilities, pipeline cache management, and model path resolution.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use tracing::{info, warn};
use web_rwkv::runtime::model::{ModelInfo, Quant};

/// Compute the pipeline cache path for a model file.
pub fn get_pipeline_cache_path(model_path: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_path.hash(&mut hasher);
    let hash = hasher.finish();
    let root = env::var("RWKV_PIPELINE_CACHE_DIR")
        .unwrap_or_else(|_| "/tmp/roco-pipeline-cache".to_string());
    PathBuf::from(root).join(format!("{:016x}.bin", hash))
}

/// Compute the quant cache directory for a model file.
pub fn get_quant_cache_dir(model_path: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_path.hash(&mut hasher);
    let hash = hasher.finish();
    let root = env::var("RWKV_QUANT_CACHE_DIR")
        .unwrap_or_else(|_| "/tmp/roco-quant-cache".to_string());
    PathBuf::from(root).join(format!("{:016x}", hash))
}

/// Auto-pick a quantization plan from the on-disk model size and GPU caps.
pub fn auto_quant(
    info: &ModelInfo,
    model_path: &str,
    _model_data: &[u8],
    gpu_coop: bool,
    _gpu_max_mb: u64,
) -> HashMap<usize, Quant> {
    let num_layer = info.num_layer as u64;
    let num_emb = info.num_emb as u64;
    let num_vocab = info.num_vocab as u64;
    let ffn_hidden = num_emb * 4;

    let params = (num_emb * num_vocab) + num_layer * (num_emb * num_emb + 2 * num_emb * ffn_hidden);
    let fp16_total_mb = (params * 2) / (1024 * 1024);
    let on_disk_mb = std::fs::metadata(model_path)
        .map(|m| m.len() / (1024 * 1024))
        .unwrap_or(fp16_total_mb);

    let quantize_threshold_mb = 1536;

    if on_disk_mb < quantize_threshold_mb {
        info!(on_disk_mb, num_layer, num_emb, "small model — no quantization");
        return HashMap::new();
    }

    // RWKV_QUANT=proxy → proxy-guided per-layer quantization
    if let Ok(mode) = env::var("RWKV_QUANT") {
        if mode == "proxy" {
            return proxy_guided_quant(info, model_path, gpu_coop);
        }
    }

    // Sandwich quantization: keep edge layers at FP16
    let n = info.num_layer;
    let edge = if n <= 4 { 0 } else { 2 };
    let q_mid = if gpu_coop { Quant::NF4 } else { Quant::Int8 };
    let mid_label = if gpu_coop { "NF4" } else { "Int8" };

    let mut plan = HashMap::new();
    for l in 0..n {
        let q = if (l as usize) < edge || (l as usize) >= n - edge {
            Quant::None
        } else {
            q_mid
        };
        plan.insert(l as usize, q);
    }

    info!(
        on_disk_mb, gpu_coop, num_layer, edge_layers = edge,
        "sandwich quantization: {edge} edge layers FP16, middle {} layers {mid_label}",
        n - 2 * edge
    );
    plan
}

/// RWKVQuant-style proxy-guided quantization.
pub fn proxy_guided_quant(
    info: &ModelInfo,
    model_path: &str,
    gpu_coop: bool,
) -> HashMap<usize, Quant> {
    use crate::quant::{analyze_model_streaming, QuantRecommendation};
    use std::collections::HashSet;

    let n = info.num_layer;
    let q = if gpu_coop { Quant::NF4 } else { Quant::Int8 };
    let q_label = if gpu_coop { "NF4" } else { "Int8" };

    info!("RWKV_QUANT=proxy — analysing weight distributions (streaming)…");
    let analysis = match analyze_model_streaming(model_path) {
        Ok(a) => a,
        Err(e) => {
            warn!("proxy analysis failed ({e}), falling back to sandwich quantization");
            return sandwich_quant(info, gpu_coop);
        }
    };
    analysis.print();

    let mut plan = HashMap::new();
    let mut layer_scores: Vec<(usize, f64)> = Vec::with_capacity(n);
    for layer in 0..n {
        let layer_tensors: Vec<_> = analysis.tensors.iter()
            .filter(|t| extract_layer_from_name(&t.name) == Some(layer))
            .collect();
        if layer_tensors.is_empty() {
            layer_scores.push((layer, 0.0));
            continue;
        }
        let total_elements: usize = layer_tensors.iter().map(|t| t.numels).sum();
        let sq_elements: usize = layer_tensors.iter()
            .filter(|t| t.recommendation == QuantRecommendation::ScalarQuant)
            .map(|t| t.numels)
            .sum();
        let score = if total_elements > 0 { sq_elements as f64 / total_elements as f64 } else { 0.0 };
        layer_scores.push((layer, score));
    }

    let fp16_budget = (n as f64 * 0.25) as usize;
    layer_scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut fp16_set: HashSet<usize> = HashSet::new();
    for (layer, _score) in layer_scores.iter().take(fp16_budget) {
        fp16_set.insert(*layer);
    }
    for l in 0..n {
        if l < 2 || l >= n - 2 { fp16_set.insert(l); }
    }

    let mut sq_layers = 0;
    for layer in 0..n {
        if !fp16_set.contains(&layer) {
            plan.insert(layer, q);
            sq_layers += 1;
        }
    }

    info!(sq = sq_layers, fp16 = fp16_set.len(), total = n,
        "proxy-guided quant: {sq_layers}/{n} layers → {q_label}, {} layers → FP16", fp16_set.len());
    plan
}

/// Fallback sandwich quantization.
pub fn sandwich_quant(info: &ModelInfo, gpu_coop: bool) -> HashMap<usize, Quant> {
    let n = info.num_layer;
    let edge = if n <= 4 { 0 } else { 2 };
    let q_mid = if gpu_coop { Quant::NF4 } else { Quant::Int8 };
    let mut plan = HashMap::new();
    for l in 0..n {
        if (l as usize) < edge || (l as usize) >= n - edge {
            plan.insert(l as usize, Quant::None);
        } else {
            plan.insert(l as usize, q_mid);
        }
    }
    plan
}

/// Extract layer index from a tensor name (`blk.{N}.*` or `blocks.{N}.*`).
pub fn extract_layer_from_name(name: &str) -> Option<usize> {
    let parts: Vec<&str> = name.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if (*part == "blk" || *part == "blocks") && i + 1 < parts.len() {
            return parts[i + 1].parse().ok();
        }
    }
    None
}

/// Resolve the default model path when `RWKV_MODEL` is unset.
pub fn default_model_path() -> anyhow::Result<PathBuf> {
    let dir = std::env::current_dir().unwrap_or_default();

    let mut search_dirs: Vec<PathBuf> = Vec::new();
    for candidate in ["models", "../models"] {
        let p = dir.join(candidate);
        if p.is_dir() { search_dirs.push(p); }
    }
    if search_dirs.is_empty() {
        anyhow::bail!(
            "no models/ directory found (tried {dir:?}models and {dir:?}../models). \
             Set $RWKV_MODEL explicitly or place a rwkv7 .st file in models/."
        );
    }

    let mut best: Option<(i32, PathBuf)> = None;
    for search_dir in &search_dirs {
        let entries = match std::fs::read_dir(search_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in entries.flatten() {
            let path = e.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if !name.starts_with("rwkv7") || !name.ends_with(".st") { continue; }
            let score = if name.contains("-converted") { 90 } else { 100 };
            if score == 0 { continue; }
            match &best {
                Some((s, _)) if *s >= score => {}
                _ => best = Some((score, path)),
            }
        }
    }

    match best {
        Some((_score, path)) => Ok(path),
        None => {
            let mut listing = String::new();
            for search_dir in &search_dirs {
                if let Ok(entries) = std::fs::read_dir(search_dir) {
                    for e in entries.flatten() {
                        if let Some(_name) = e.path().file_name().and_then(|n| n.to_str()) {
                            listing.push_str(&format!("  {} ({})\n",
                                e.path().display(),
                                std::fs::metadata(e.path()).map(|m| format!("{}MB", m.len() / (1024 * 1024))).unwrap_or_default()
                            ));
                        }
                    }
                }
            }
            anyhow::bail!(
                "no rwkv7 .st file found in any of {:?}.\nModels on disk:\n{listing}\n\
                 Hint: convert a GGUF to SafeTensors first (scripts/convert_gguf_to_st.py), \
                 or set $RWKV_MODEL explicitly.",
                search_dirs
            )
        }
    }
}
