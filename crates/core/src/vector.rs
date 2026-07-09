//! FAISS-style vector index for RAG / semantic retrieval.
//!
//! Pure Rust, zero ML dependencies — an exact brute-force cosine search over
//! an in-memory index. For "very small, very fast" corpora (thousands of
//! items) this is competitive with FAISS's flat index and needs no native
//! library. A pluggable [`Embedder`] produces the vectors; [`HashingEmbedder`]
//! is a dependency-free default for dev/tests and tiny corpora.
//!
//! Swap-in note: an HNSW/IVF index or the real `faiss` crate can back the same
//! [`VectorStore`] API later behind a feature flag, keeping tool callers unchanged.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// A dense embedding vector.
pub type Embedding = Vec<f32>;

/// A stored item plus its similarity score to a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredHit {
    pub id: String,
    /// Cosine similarity in [-1, 1].
    pub score: f32,
    pub payload: Option<Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },
}

/// In-memory cosine index (FAISS `IndexFlatIP` equivalent).
#[derive(Debug, Default)]
pub struct VectorStore {
    dim: usize,
    entries: Vec<Entry>,
}

#[derive(Debug, Clone)]
struct Entry {
    id: String,
    vec: Embedding,
    payload: Option<Value>,
}

impl VectorStore {
    pub fn new(dim: usize) -> Self {
        Self {
            dim: dim.max(1),
            entries: Vec::new(),
        }
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Store `vec` under `id` (replacing any prior entry with the same id).
    /// `vec.len()` must match [`VectorStore::dim`].
    pub fn add(
        &mut self,
        id: &str,
        vec: Embedding,
        payload: Option<Value>,
    ) -> Result<(), VectorError> {
        if vec.len() != self.dim {
            return Err(VectorError::DimensionMismatch {
                expected: self.dim,
                got: vec.len(),
            });
        }
        // Replace any prior entry with this id to keep the index authoritative.
        self.entries.retain(|e| e.id != id);
        self.entries.push(Entry {
            id: id.to_string(),
            vec,
            payload,
        });
        Ok(())
    }

    /// Remove the entry with `id` (if present). Used by conflict resolution
    /// when a newer memory supersedes/deletes an older one.
    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|e| e.id != id);
    }

    /// Top-`k` nearest neighbours by cosine similarity (exact, descending).
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<ScoredHit>, VectorError> {
        if query.len() != self.dim {
            return Err(VectorError::DimensionMismatch {
                expected: self.dim,
                got: query.len(),
            });
        }
        if self.entries.is_empty() {
            return Ok(Vec::new());
        }
        let mut scored: Vec<ScoredHit> = self
            .entries
            .iter()
            .map(|e| ScoredHit {
                id: e.id.clone(),
                score: cosine_similarity(query, &e.vec),
                payload: e.payload.clone(),
            })
            .collect();
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let k = k.min(scored.len());
        scored.truncate(k);
        Ok(scored)
    }
}

/// Cosine similarity of two equally-sized vectors (no prior normalization needed).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len().min(b.len()) {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = na.max(0.0).sqrt() * nb.max(0.0).sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// L2-normalize a vector in place.
pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// Produces an [`Embedding`] for text. Swap in a real sentence-transformer /
/// GGUF embedder later; the hashing default needs no model.
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Embedding;
}

/// Deterministic, dependency-free embedder for dev, tests, and tiny corpora.
/// Hashes character n-grams into bins and L2-normalizes, so cosine similarity
/// approximates lexical overlap. Fast and allocation-light.
pub struct HashingEmbedder {
    dim: usize,
}

impl HashingEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim: dim.max(1) }
    }
}

impl Embedder for HashingEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Embedding {
        let mut v = vec![0.0f32; self.dim];
        let bytes = text.to_lowercase().into_bytes();
        if !bytes.is_empty() {
            for n in 1..=3 {
                if bytes.len() < n {
                    continue;
                }
                for gram in bytes.windows(n) {
                    let h = fnv1a(gram) as usize % self.dim;
                    v[h] += 1.0;
                }
            }
        }
        normalize(&mut v);
        v
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// A `VectorStore` guarded by a `Mutex` so it can be shared across tool calls
/// inside an `Arc<dyn Tool>` (stateful RAG).
pub type SharedVectorStore = Arc<Mutex<VectorStore>>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_wrong_dimension() {
        let mut s = VectorStore::new(4);
        assert!(matches!(
            s.add("x", vec![0.0; 3], None),
            Err(VectorError::DimensionMismatch { .. })
        ));
        assert!(matches!(
            s.search(&[0.0; 3], 1),
            Err(VectorError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn search_returns_nearest_first() {
        let mut s = VectorStore::new(4);
        s.add("a", vec![1.0, 0.0, 0.0, 0.0], Some(json!({"n": 1})))
            .unwrap();
        s.add("b", vec![0.0, 1.0, 0.0, 0.0], None).unwrap();
        s.add("c", vec![0.0, 0.0, 1.0, 0.0], None).unwrap();
        let hits = s.search(&[1.0, 0.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "a");
        assert!((hits[0].score - 1.0).abs() < 1e-5);
    }

    #[test]
    fn empty_store_searches_to_nothing() {
        let s = VectorStore::new(4);
        assert!(s.search(&[1.0; 4], 5).unwrap().is_empty());
    }

    #[test]
    fn hashing_embedder_is_deterministic_and_normalized() {
        let e = HashingEmbedder::new(64);
        let v1 = e.embed("the cat sat");
        let v2 = e.embed("the cat sat");
        assert_eq!(v1, v2);
        let norm = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn similar_text_scores_higher_than_dissimilar() {
        let e = HashingEmbedder::new(128);
        let q = e.embed("cat sits on mat");
        let near = e.embed("the cat sat on the mat");
        let far = e.embed("quantum entanglement superconductors");
        let s_near = cosine_similarity(&q, &near);
        let s_far = cosine_similarity(&q, &far);
        assert!(s_near > s_far, "{s_near} > {s_far}");
    }
}
