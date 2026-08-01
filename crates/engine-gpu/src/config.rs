//! Model configuration, quantization planning, and path resolution.
//!
//! Provides automatic quantization selection based on model size and GPU
//! capabilities, pipeline cache management, and model path resolution.

use std::collections::HashMap;
use std::env;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use web_rwkv::runtime::model::{ModelInfo, Quant};

/// Default root for all model caches.
fn default_cache_root() -> PathBuf {
    if let Ok(dir) = env::var("RWKV_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    // Persistent location: ~/.cache/roco/ (not /tmp/ — /tmp/ is wiped on reboot)
    if let Ok(home) = env::var("HOME") {
        PathBuf::from(home).join(".cache").join("roco")
    } else {
        // Fallback (no HOME set)
        PathBuf::from("/tmp/roco-cache")
    }
}

/// Hash a model file stably by its filesystem identity (device:inode).
///
/// This ensures the same physical model file produces the same cache key
/// regardless of whether it's referenced by a relative path, an absolute
/// path, or through a symlink.
///
/// Falls back to hashing the canonicalised path (or the raw path as last
/// resort) if `stat` fails.
pub fn model_file_hash(model_path: &str) -> u64 {
    let path = Path::new(model_path);

    // Stable content-based hash: device + inode + size
    #[cfg(target_os = "linux")]
    if let Ok(meta) = std::fs::metadata(path) {
        use std::os::linux::fs::MetadataExt;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        meta.st_dev().hash(&mut hasher);
        meta.st_ino().hash(&mut hasher);
        meta.len().hash(&mut hasher);
        return hasher.finish();
    }

    // Fallback 1: canonicalise to resolve symlinks
    if let Ok(canon) = path.canonicalize() {
        let s = canon.to_string_lossy();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut hasher);
        return hasher.finish();
    }

    // Fallback 2: hash the raw path
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_path.hash(&mut hasher);
    hasher.finish()
}

/// Compute the pipeline cache path for a model file.
pub fn get_pipeline_cache_path(model_path: &str) -> PathBuf {
    let hash = model_file_hash(model_path);
    let root = env::var("RWKV_PIPELINE_CACHE_DIR").unwrap_or_else(|_| {
        default_cache_root()
            .join("pipeline-cache")
            .to_string_lossy()
            .to_string()
    });
    PathBuf::from(root).join(format!("{:016x}.bin", hash))
}

/// Compute the quant cache directory for a model file.
pub fn get_quant_cache_dir(model_path: &str) -> PathBuf {
    let hash = model_file_hash(model_path);
    let root = env::var("RWKV_QUANT_CACHE_DIR").unwrap_or_else(|_| {
        default_cache_root()
            .join("quant-cache")
            .to_string_lossy()
            .to_string()
    });
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
        info!(
            on_disk_mb,
            num_layer, num_emb, "small model — no quantization"
        );
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
        let q = if l < edge || l >= n - edge {
            Quant::None
        } else {
            q_mid
        };
        plan.insert(l, q);
    }

    info!(
        on_disk_mb,
        gpu_coop,
        num_layer,
        edge_layers = edge,
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
        let layer_tensors: Vec<_> = analysis
            .tensors
            .iter()
            .filter(|t| extract_layer_from_name(&t.name) == Some(layer))
            .collect();
        if layer_tensors.is_empty() {
            layer_scores.push((layer, 0.0));
            continue;
        }
        let total_elements: usize = layer_tensors.iter().map(|t| t.numels).sum();
        let sq_elements: usize = layer_tensors
            .iter()
            .filter(|t| t.recommendation == QuantRecommendation::ScalarQuant)
            .map(|t| t.numels)
            .sum();
        let score = if total_elements > 0 {
            sq_elements as f64 / total_elements as f64
        } else {
            0.0
        };
        layer_scores.push((layer, score));
    }

    let fp16_budget = (n as f64 * 0.25) as usize;
    layer_scores.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut fp16_set: HashSet<usize> = HashSet::new();
    for (layer, _score) in layer_scores.iter().take(fp16_budget) {
        fp16_set.insert(*layer);
    }
    for l in 0..n {
        if l < 2 || l >= n - 2 {
            fp16_set.insert(l);
        }
    }

    let mut sq_layers = 0;
    for layer in 0..n {
        if !fp16_set.contains(&layer) {
            plan.insert(layer, q);
            sq_layers += 1;
        }
    }

    info!(
        sq = sq_layers,
        fp16 = fp16_set.len(),
        total = n,
        "proxy-guided quant: {sq_layers}/{n} layers → {q_label}, {} layers → FP16",
        fp16_set.len()
    );
    plan
}

/// Fallback sandwich quantization.
pub fn sandwich_quant(info: &ModelInfo, gpu_coop: bool) -> HashMap<usize, Quant> {
    let n = info.num_layer;
    let edge = if n <= 4 { 0 } else { 2 };
    let q_mid = if gpu_coop { Quant::NF4 } else { Quant::Int8 };
    let mut plan = HashMap::new();
    for l in 0..n {
        if l < edge || l >= n - edge {
            plan.insert(l, Quant::None);
        } else {
            plan.insert(l, q_mid);
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

// ---------------------------------------------------------------------------
// Cache index — tracks which tensors are cached on disk.
// ---------------------------------------------------------------------------

/// A tensor entry in the cache index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTensorInfo {
    /// Tensor name (e.g. "blocks.0.att.key.weight")
    pub name: String,
    /// Data type ("F16", "F32", etc.)
    pub dtype: String,
    /// Shape
    pub shape: Vec<usize>,
    /// Whether this tensor is cached as quantized (true) or raw (false)
    pub quantized: bool,
    /// md5 hex of the original tensor bytes for integrity check
    pub md5: Option<String>,
}

/// The cache index JSON — saved after a successful first load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheIndex {
    /// Model file name (for display)
    pub model_name: String,
    /// All tensors that are cached on disk
    pub tensors: Vec<CachedTensorInfo>,
    /// Model metadata snapshot
    pub num_layer: usize,
    pub num_emb: usize,
    pub num_hidden: usize,
    pub num_vocab: usize,
    pub num_head: usize,
    pub model_version: String,
}

impl CacheIndex {
    /// Check if the cache index exists and is valid.
    pub fn load(quant_cache_dir: &std::path::Path) -> Option<Self> {
        let path = quant_cache_dir.join("cache_index.json");
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Save the cache index to disk.
    pub fn save(&self, quant_cache_dir: &std::path::Path) {
        let path = quant_cache_dir.join("cache_index.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => match std::fs::write(&path, &json) {
                Ok(()) => info!(path = ?path, "saved cache index"),
                Err(e) => warn!(path = ?path, error = %e, "failed to save cache index"),
            },
            Err(e) => warn!(error = %e, "failed to serialize cache index"),
        }
    }

    /// Check if all tensors listed in the index actually exist on disk.
    pub fn is_complete(&self, quant_cache_dir: &std::path::Path) -> bool {
        for t in &self.tensors {
            let safe = t.name.replace(['.', '/', '\\', ':'], "_");
            if t.quantized {
                // Quantized matrices have _q.bin and _m.bin (and _scale.bin for NF4)
                let q_path = quant_cache_dir.join(format!("{safe}_q.bin"));
                if !q_path.exists() {
                    warn!(tensor = %t.name, "missing quantized tensor cache file");
                    return false;
                }
            } else {
                // Raw (vector) cache has _vec.bin
                let vec_path = quant_cache_dir.join(format!("{safe}_vec.bin"));
                if !vec_path.exists() {
                    warn!(tensor = %t.name, "missing vector cache file");
                    return false;
                }
            }
        }
        info!(
            "cache index complete: {} tensors cached in {:?}",
            self.tensors.len(),
            quant_cache_dir
        );
        true
    }
}

/// Compute the old path-based hash for a model path.
fn old_path_hash(model_path: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_path.hash(&mut hasher);
    hasher.finish()
}

/// Compute the old path-based quant cache directory (used before the content-hash migration).
fn old_get_quant_cache_dir(model_path: &str) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    model_path.hash(&mut hasher);
    let hash = hasher.finish();
    let root = env::var("RWKV_QUANT_CACHE_DIR").unwrap_or_else(|_| {
        default_cache_root()
            .join("quant-cache")
            .to_string_lossy()
            .to_string()
    });
    PathBuf::from(root).join(format!("{:016x}", hash))
}

/// Migrate an old path-based quant cache to the new content-based hash.
///
/// If the new location is empty and an old cache exists and is complete,
/// rename the old directory to the new hash. This preserves the cached
/// quantisation across the migration.
pub fn migrate_quant_cache(model_path: &str) {
    let new_dir = get_quant_cache_dir(model_path);
    if new_dir.exists() {
        return; // already migrated or fresh
    }

    // Try the old path-based hash via the canonicalised path (most likely match)
    let canonicalised = Path::new(model_path).canonicalize().ok();
    let old_candidates = std::iter::once(model_path.to_string()).chain(
        canonicalised
            .as_ref()
            .map(|c| c.to_string_lossy().to_string()),
    );

    for old_path in old_candidates {
        let old_dir = old_get_quant_cache_dir(&old_path);
        if !old_dir.exists() {
            continue;
        }
        // Check if old cache is complete
        if let Some(index) = CacheIndex::load(&old_dir) {
            if index.is_complete(&old_dir) {
                info!("migrating quant cache: {:?} -> {:?}", old_dir, new_dir);
                if let Err(e) = std::fs::rename(&old_dir, &new_dir) {
                    warn!(error = %e, "failed to migrate quant cache");
                }
                return;
            }
        }
    }
}

/// Check if a complete model cache exists for the given model path.
/// Returns `Some(CacheIndex)` if cache is complete, `None` otherwise.
pub fn check_model_cache(model_path: &str) -> Option<CacheIndex> {
    // Try the new content-based cache location
    let quant_cache_dir = get_quant_cache_dir(model_path);
    if quant_cache_dir.exists() {
        if let Some(index) = CacheIndex::load(&quant_cache_dir) {
            if index.is_complete(&quant_cache_dir) {
                return Some(index);
            }
        }
    }
    // Fallback: attempt migration from old path-based cache
    migrate_quant_cache(model_path);
    // Retry after migration
    if quant_cache_dir.exists() {
        let index = CacheIndex::load(&quant_cache_dir)?;
        if index.is_complete(&quant_cache_dir) {
            return Some(index);
        }
    }
    None
}

/// Migrate an old path-based pipeline cache to the new content-based hash.
pub fn migrate_pipeline_cache(model_path: &str) {
    let new_path = get_pipeline_cache_path(model_path);
    if new_path.exists() {
        return; // already migrated or fresh
    }

    let canonicalised = Path::new(model_path).canonicalize().ok();
    let old_candidates = std::iter::once(model_path.to_string()).chain(
        canonicalised
            .as_ref()
            .map(|c| c.to_string_lossy().to_string()),
    );

    for old_path in old_candidates {
        let old_hash = old_path_hash(&old_path);
        let root = env::var("RWKV_PIPELINE_CACHE_DIR").unwrap_or_else(|_| {
            default_cache_root()
                .join("pipeline-cache")
                .to_string_lossy()
                .to_string()
        });
        let old_file = PathBuf::from(root).join(format!("{:016x}.bin", old_hash));
        if old_file.exists() {
            info!("migrating pipeline cache: {:?} -> {:?}", old_file, new_path);
            if let Err(e) = std::fs::rename(&old_file, &new_path) {
                warn!(error = %e, "failed to migrate pipeline cache");
            }
            return;
        }
    }
}

/// Compute md5 of a byte slice.
pub fn compute_md5(data: &[u8]) -> Option<String> {
    use md5::{Digest, Md5};
    use std::fmt::Write;
    let result = Md5::digest(data);
    let mut hex = String::with_capacity(32);
    for b in result {
        write!(hex, "{:02x}", b).ok();
    }
    Some(hex)
}

/// Resolve the default model path when `RWKV_MODEL` is unset.
pub fn default_model_path() -> anyhow::Result<PathBuf> {
    let dir = std::env::current_dir().unwrap_or_default();

    let mut search_dirs: Vec<PathBuf> = Vec::new();
    for candidate in ["models", "../models"] {
        let p = dir.join(candidate);
        if p.is_dir() {
            search_dirs.push(p);
        }
    }

    // Also scan user cache directories like ~/.cache/roco
    if let Ok(home) = env::var("HOME") {
        let p_home_cache = PathBuf::from(home).join(".cache").join("roco");
        if p_home_cache.is_dir() {
            search_dirs.push(p_home_cache);
        }
    }

    if search_dirs.is_empty() {
        anyhow::bail!(
            "no models/ or ~/.cache/roco/ directory found (tried {dir:?}models, {dir:?}../models, and ~/.cache/roco/).\n\
             Hint: set RWKV_MODEL and check docs/rwkv-v7-g1.md"
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
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".st") {
                continue;
            }
            let lower = name.to_lowercase();
            // Score: prefer RWKV-7 models over generic .st files
            let score = if lower.contains("rwkv") && lower.contains("7") {
                if lower.contains("-converted") {
                    90
                } else {
                    100
                }
            } else if lower.contains("rwkv") {
                80
            } else {
                10
            };
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
                "no .st model file found in any of {:?}.\nFiles on disk:\n{listing}\n\
                 Hint: set RWKV_MODEL and check docs/rwkv-v7-g1.md",
                search_dirs
            )
        }
    }
}
