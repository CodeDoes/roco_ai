//! RNN-powered memory processor (RWKV7-linear-attention backend).
//!
//! Goes beyond chat-history RAG: it *extracts* facts, *deduplicates*,
//! *resolves conflicts*, and *models user state* — recreating the Mem0,
//! Honcho, Letta/MemGPT, and Zep patterns from scratch.
//!
//! The LLM steps (fact extraction, conflict resolution, dialectic synthesis,
//! triple extraction) are driven through [`ModelBackend`], so the RWKV7-g1g
//! (1.5B/2.9B) model powers them once a local backend exists. Every step also
//! has a **deterministic fallback** (hashing embedder + heuristic rules) so the
//! whole system is runnable and tested today with no model — "very small, very
//! fast".

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::{CompletionRequest, ModelBackend};
use crate::tools::{Tool, ToolError};
use crate::vector::{cosine_similarity, Embedder, SharedVectorStore, VectorStore};

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("memory backend error: {0}")]
    Backend(String),
    #[error("memory parse error: {0}")]
    Parse(String),
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ===========================================================================
// Shared prompts (tuned for small RWKV7 instruction-following)
// ===========================================================================

const FACT_EXTRACTION_SYSTEM: &str = "You extract salient, durable facts about a user from a \
message. Ignore conversational filler, greetings, and transient statements. Return ONLY a JSON \
array of short strings, e.g. [\"User moved to Austin, TX\", \"User has a dog named Barnaby\"]. \
If no durable facts, return [].";

const CONFLICT_SYSTEM: &str = "You are a memory conflict resolver. Given a NEW memory and the \
EXISTING memories, decide ONE action: ADD (new and not already known), UPDATE (refines or changes \
an existing memory), DELETE (new memory contradicts an existing one and is more recent/true), or \
NONE (already known). Respond with ONLY JSON: {\"action\": \"ADD|UPDATE|DELETE|NONE\", \
\"target_id\": \"<id of the existing fact to update/delete, or null>\"}.";

const DIALECTIC_SYSTEM: &str = "You are a dialectic reasoning engine modeling a user's evolving \
state. Given the CURRENT state and the recent conversation, produce a REVISED state JSON with \
fields: identity, current_goals, preferences, open_loops. Reflect how the user changed. Output \
ONLY JSON.";

const TRIPLE_SYSTEM: &str = "Extract subject-predicate-object triples from the text as a JSON \
array of [subject, predicate, object]. Output ONLY JSON, e.g. [[\"User\", \"lives_in\", \
\"Austin\"]]. If none, return [].";

// ===========================================================================
// Mem0 — fact store with extract / dedup / conflict resolution
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub user_id: String,
    pub text: String,
    pub created_at: u64,
    pub updated_at: u64,
    /// Salience 0..1 (currently 1.0; a real model can score it).
    pub score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    Add,
    Update,
    Delete,
    None,
}

/// The decision returned by conflict resolution, including the targeted fact id.
#[derive(Debug, Clone)]
pub struct Decision {
    pub resolution: Resolution,
    pub target_id: Option<String>,
}

/// Mem0-style fact store: discrete facts scoped by `user_id`, backed by a
/// vector index for semantic dedup/retrieval.
pub struct MemoryStore {
    facts: HashMap<String, Fact>,
    by_user: HashMap<String, Vec<String>>,
    index: SharedVectorStore,
    embedder: Arc<dyn Embedder>,
    next_id: u64,
}

impl MemoryStore {
    pub fn new(embedder: Arc<dyn Embedder>) -> Self {
        let dim = embedder.dim();
        Self {
            facts: HashMap::new(),
            by_user: HashMap::new(),
            index: Arc::new(Mutex::new(VectorStore::new(dim))),
            embedder,
            next_id: 0,
        }
    }

    pub fn add_fact(&mut self, user_id: &str, text: &str) -> Fact {
        let id = format!("f{}", self.next_id);
        self.next_id += 1;
        let now = now_ms();
        let fact = Fact {
            id: id.clone(),
            user_id: user_id.to_string(),
            text: text.to_string(),
            created_at: now,
            updated_at: now,
            score: 1.0,
        };
        let vec = self.embedder.embed(text);
        self.index
            .lock()
            .unwrap()
            .add(&id, vec, Some(serde_json::json!({ "text": text })))
            .ok();
        self.facts.insert(id.clone(), fact.clone());
        self.by_user
            .entry(user_id.to_string())
            .or_default()
            .push(id);
        fact
    }

    pub fn update_fact(&mut self, id: &str, text: &str) -> Option<Fact> {
        let updated = {
            let f = self.facts.get_mut(id)?;
            f.text = text.to_string();
            f.updated_at = now_ms();
            f.clone()
        };
        let vec = self.embedder.embed(text);
        self.index
            .lock()
            .unwrap()
            .add(id, vec, Some(serde_json::json!({ "text": text })))
            .ok();
        Some(updated)
    }

    pub fn delete_fact(&mut self, id: &str) {
        if let Some(f) = self.facts.remove(id) {
            if let Some(ids) = self.by_user.get_mut(&f.user_id) {
                ids.retain(|x| x != id);
            }
            self.index.lock().unwrap().remove(id);
        }
    }

    pub fn facts_for(&self, user_id: &str) -> Vec<Fact> {
        self.by_user
            .get(user_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|i| self.facts.get(i).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Top-`k` most similar facts for `user_id` (semantic dedup/retrieval).
    pub fn search(&self, user_id: &str, query: &str, k: usize) -> Vec<Fact> {
        let vec = self.embedder.embed(query);
        let hits = self
            .index
            .lock()
            .unwrap()
            .search(&vec, (k * 3).max(10))
            .unwrap_or_default();
        hits.iter()
            .filter_map(|h| self.facts.get(&h.id))
            .filter(|f| f.user_id == user_id)
            .take(k)
            .cloned()
            .collect()
    }

    /// Deterministic conflict decision: duplicate -> None, near-duplicate ->
    /// Update, otherwise Add. (DELETE requires recency/truth, so the model does
    /// it; the fallback never deletes.)
    pub fn deterministic_resolve(&self, new_text: &str, candidates: &[Fact]) -> Decision {
        if candidates.is_empty() {
            return Decision {
                resolution: Resolution::Add,
                target_id: None,
            };
        }
        let nv = self.embedder.embed(new_text);
        let mut best = 0.0f32;
        let mut best_id: Option<String> = None;
        for c in candidates {
            let cv = self.embedder.embed(&c.text);
            let s = cosine_similarity(&nv, &cv);
            if s > best {
                best = s;
                best_id = Some(c.id.clone());
            }
        }
        let resolution = if best > 0.95 {
            Resolution::None
        } else if best > 0.7 {
            Resolution::Update
        } else {
            Resolution::Add
        };
        Decision {
            resolution,
            target_id: best_id,
        }
    }

    fn apply(&mut self, user_id: &str, text: &str, d: &Decision) {
        let target = d
            .target_id
            .clone()
            .or_else(|| self.facts_for(user_id).first().map(|f| f.id.clone()));
        match d.resolution {
            Resolution::Add => {
                self.add_fact(user_id, text);
            }
            Resolution::None => {}
            Resolution::Update => {
                if let Some(id) = target {
                    if self.facts.contains_key(&id) {
                        self.update_fact(&id, text);
                        return;
                    }
                }
                self.add_fact(user_id, text);
            }
            Resolution::Delete => {
                if let Some(id) = target {
                    self.delete_fact(&id);
                }
            }
        }
    }
}

// ===========================================================================
// Honcho — dynamic user state + dialectic "dreaming"
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UserState {
    pub identity: String,
    pub current_goals: String,
    pub preferences: String,
    pub open_loops: String,
    pub updated_at: u64,
}

// ===========================================================================
// Zep — temporal knowledge graph
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalEdge {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: u64,
    pub valid_to: Option<u64>,
    pub source: String,
}

/// Zep-style temporal graph: edges carry `valid_from`/`valid_to`; a new edge
/// with the same (subject, predicate) closes the previously-current one.
#[derive(Debug, Default, Clone)]
pub struct TemporalGraph {
    edges: Vec<TemporalEdge>,
}

impl TemporalGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn merge(&mut self, new: Vec<TemporalEdge>) {
        let now = now_ms();
        for e in new {
            for ex in self.edges.iter_mut() {
                if ex.subject.eq_ignore_ascii_case(&e.subject)
                    && ex.predicate.eq_ignore_ascii_case(&e.predicate)
                    && ex.valid_to.is_none()
                {
                    ex.valid_to = Some(now);
                }
            }
            self.edges.push(e);
        }
    }

    /// Currently-valid edges (valid_to is None).
    pub fn current(&self) -> Vec<&TemporalEdge> {
        self.edges.iter().filter(|e| e.valid_to.is_none()).collect()
    }

    pub fn current_for(&self, subject: &str) -> Vec<&TemporalEdge> {
        self.current()
            .into_iter()
            .filter(|e| e.subject.eq_ignore_ascii_case(subject))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }
}

// ===========================================================================
// Letta / MemGPT — tiered, self-paged memory
// ===========================================================================

#[derive(Debug, Clone, Default)]
pub struct LettaMemory {
    /// Always in the system prompt (the "RAM").
    pub core: String,
    /// Capped rolling log of raw conversation (recall memory).
    pub recall: Vec<String>,
    /// Long-term facts/docs (archival memory, semantic).
    pub archival: SharedVectorStore,
    dim: usize,
}

impl LettaMemory {
    pub fn new(embedder: &dyn Embedder) -> Self {
        Self {
            core: String::new(),
            recall: Vec::new(),
            archival: Arc::new(Mutex::new(VectorStore::new(embedder.dim()))),
            dim: embedder.dim(),
        }
    }

    pub fn core_replace(&mut self, old: &str, new: &str) {
        self.core = self.core.replace(old, new);
    }

    pub fn core_append(&mut self, s: &str) {
        if !self.core.is_empty() {
            self.core.push('\n');
        }
        self.core.push_str(s);
    }

    pub fn recall_append(&mut self, s: &str, cap: usize) {
        self.recall.push(s.to_string());
        while self.recall.len() > cap {
            self.recall.remove(0);
        }
    }

    pub fn archival_add(&self, embedder: &dyn Embedder, text: &str) {
        let id = format!("a{}", now_ms());
        let v = embedder.embed(text);
        self.archival
            .lock()
            .unwrap()
            .add(&id, v, Some(serde_json::json!({ "text": text })))
            .ok();
    }

    pub fn archival_search(&self, embedder: &dyn Embedder, query: &str, k: usize) -> Vec<String> {
        let v = embedder.embed(query);
        self.archival
            .lock()
            .unwrap()
            .search(&v, k.max(1))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|h| {
                h.payload.and_then(|p| {
                    p.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                })
            })
            .collect()
    }

    pub fn dim(&self) -> usize {
        self.dim
    }
}

// ===========================================================================
// MemoryProcessor — ties the patterns together, model-driven + fallbacks
// ===========================================================================

pub struct MemoryProcessor {
    store: Arc<Mutex<MemoryStore>>,
    graph: Arc<Mutex<TemporalGraph>>,
    states: Arc<Mutex<HashMap<String, UserState>>>,
    letta: Arc<Mutex<HashMap<String, LettaMemory>>>,
    history: Arc<Mutex<HashMap<String, Vec<String>>>>,
    embedder: Arc<dyn Embedder>,
}

impl MemoryProcessor {
    pub fn new(embedder: Arc<dyn Embedder>) -> Self {
        Self {
            store: Arc::new(Mutex::new(MemoryStore::new(embedder.clone()))),
            graph: Arc::new(Mutex::new(TemporalGraph::new())),
            states: Arc::new(Mutex::new(HashMap::new())),
            letta: Arc::new(Mutex::new(HashMap::new())),
            history: Arc::new(Mutex::new(HashMap::new())),
            embedder,
        }
    }

    pub fn embedder(&self) -> Arc<dyn Embedder> {
        self.embedder.clone()
    }

    /// Record a raw transcript turn for a user (feeds Honcho dreaming).
    pub fn record(&self, user_id: &str, text: &str) {
        self.history
            .lock()
            .unwrap()
            .entry(user_id.to_string())
            .or_default()
            .push(text.to_string());
    }

    // ----- Mem0 ingest (extract -> dedup -> resolve -> apply) -----

    pub async fn ingest<B: ModelBackend + Send + Sync + 'static>(
        &self,
        backend: &B,
        user_id: &str,
        text: &str,
    ) -> Result<Vec<Resolution>, MemoryError> {
        self.record(user_id, text);
        let facts = self.extract_facts(backend, text).await?;
        let mut out = Vec::new();
        for f in facts {
            let candidates = self.store.lock().unwrap().search(user_id, &f, 3);
            let decision = match self.resolve_with_model(backend, &f, &candidates).await {
                Ok(d) => d,
                Err(_) => self
                    .store
                    .lock()
                    .unwrap()
                    .deterministic_resolve(&f, &candidates),
            };
            self.store.lock().unwrap().apply(user_id, &f, &decision);
            out.push(decision.resolution);
        }
        Ok(out)
    }

    /// Deterministic ingest (no model): sentence-split facts + heuristic resolve.
    pub fn ingest_deterministic(&self, user_id: &str, text: &str) -> Vec<Resolution> {
        self.record(user_id, text);
        let mut out = Vec::new();
        for f in extract_facts_fallback(text) {
            let candidates = self.store.lock().unwrap().search(user_id, &f, 3);
            let d = self
                .store
                .lock()
                .unwrap()
                .deterministic_resolve(&f, &candidates);
            self.store.lock().unwrap().apply(user_id, &f, &d);
            out.push(d.resolution);
        }
        out
    }

    pub fn retrieve(&self, user_id: &str, query: &str, k: usize) -> Vec<Fact> {
        self.store.lock().unwrap().search(user_id, query, k)
    }

    async fn extract_facts<B: ModelBackend + Send + Sync + 'static>(
        &self,
        backend: &B,
        text: &str,
    ) -> Result<Vec<String>, MemoryError> {
        let resp = backend
            .complete(CompletionRequest {
                system: FACT_EXTRACTION_SYSTEM.into(),
                prompt: text.into(),
                output_schema: Some("[string]".into()),
                grammar: None,
                temperature: 0.1,
                max_tokens: 512,
                estimated_prompt_tokens: text.len() / 4,
                thinking: false,
                preserve_state: false,
                on_token: None,
            })
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        if let Ok(v) = serde_json::from_str::<Value>(&resp.text) {
            if let Some(arr) = v.as_array() {
                let facts: Vec<String> = arr
                    .iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect();
                if !facts.is_empty() {
                    return Ok(facts);
                }
            }
        }
        Ok(extract_facts_fallback(text))
    }

    async fn resolve_with_model<B: ModelBackend + Send + Sync + 'static>(
        &self,
        backend: &B,
        new_text: &str,
        candidates: &[Fact],
    ) -> Result<Decision, MemoryError> {
        let mut prompt = format!("NEW MEMORY:\n{new_text}\n\nEXISTING MEMORIES:\n");
        for (i, c) in candidates.iter().enumerate() {
            prompt.push_str(&format!("{}. [{}] {}\n", i + 1, c.id, c.text));
        }
        let est = prompt.len() / 4;
        let resp = backend
            .complete(CompletionRequest {
                system: CONFLICT_SYSTEM.into(),
                prompt,
                output_schema: Some(
                    r#"{"action":"<ADD|UPDATE|DELETE|NONE>","target_id":"<id|null>"}"#.into(),
                ),
                grammar: None,
                temperature: 0.0,
                max_tokens: 128,
                estimated_prompt_tokens: est,
                thinking: false,
                preserve_state: false,
                on_token: None,
            })
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        let v: Value =
            serde_json::from_str(&resp.text).map_err(|e| MemoryError::Parse(e.to_string()))?;
        let action = v.get("action").and_then(|a| a.as_str()).unwrap_or("ADD");
        let target = v
            .get("target_id")
            .and_then(|t| t.as_str())
            .filter(|t| !t.is_empty() && *t != "null")
            .map(|s| s.to_string());
        let resolution = match action {
            "UPDATE" => Resolution::Update,
            "DELETE" => Resolution::Delete,
            "NONE" => Resolution::None,
            _ => Resolution::Add,
        };
        Ok(Decision {
            resolution,
            target_id: target,
        })
    }

    // ----- Honcho dialectic state + dreaming -----

    pub async fn synthesize_state<B: ModelBackend + Send + Sync + 'static>(
        &self,
        backend: &B,
        user_id: &str,
        transcript: &str,
    ) -> Result<UserState, MemoryError> {
        let current = self
            .states
            .lock()
            .unwrap()
            .get(user_id)
            .cloned()
            .unwrap_or_default();
        let prompt = format!(
            "CURRENT STATE:\n{}\n\nRECENT CONVERSATION:\n{}",
            serde_json::to_string_pretty(&current).unwrap_or_default(),
            transcript
        );
        let est = prompt.len() / 4;
        let resp = backend
            .complete(CompletionRequest {
                system: DIALECTIC_SYSTEM.into(),
                prompt,
                output_schema: Some(
                    r#"{"identity":"<string>","current_goals":"<string>","preferences":"<string>","open_loops":"<string>"}"#
                        .into(),
                ),
                grammar: None,
                temperature: 0.2,
                max_tokens: 512,
                estimated_prompt_tokens: est,
                thinking: false,
            preserve_state: false,
            on_token: None,
            })
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        let state: UserState = serde_json::from_str(&resp.text)
            .map_err(|e| MemoryError::Parse(e.to_string()))
            .map(|mut s: UserState| {
                s.updated_at = now_ms();
                s
            })
            .unwrap_or_else(|_| {
                // Fallback: keep current state, log the transcript as an open loop.
                let mut s = current;
                s.open_loops = format!(
                    "{}\n{}",
                    s.open_loops,
                    transcript.lines().next().unwrap_or("")
                );
                s.updated_at = now_ms();
                s
            });
        self.states
            .lock()
            .unwrap()
            .insert(user_id.to_string(), state.clone());
        Ok(state)
    }

    /// "Dreaming": offline synthesis over the user's full session history.
    pub async fn dream<B: ModelBackend + Send + Sync + 'static>(
        &self,
        backend: &B,
        user_id: &str,
    ) -> Result<UserState, MemoryError> {
        let history = self
            .history
            .lock()
            .unwrap()
            .get(user_id)
            .cloned()
            .unwrap_or_default();
        let transcript = history.join("\n---\n");
        // Dreaming frames the same dialectic engine over the whole archive.
        self.synthesize_state(backend, user_id, &transcript).await
    }

    pub fn user_state(&self, user_id: &str) -> Option<UserState> {
        self.states.lock().unwrap().get(user_id).cloned()
    }

    // ----- Zep temporal graph -----

    pub async fn extract_triples<B: ModelBackend + Send + Sync + 'static>(
        &self,
        backend: &B,
        text: &str,
    ) -> Result<Vec<TemporalEdge>, MemoryError> {
        let resp = backend
            .complete(CompletionRequest {
                system: TRIPLE_SYSTEM.into(),
                prompt: text.into(),
                output_schema: Some("[[subject,predicate,object]]".into()),
                grammar: None,
                temperature: 0.0,
                max_tokens: 512,
                estimated_prompt_tokens: text.len() / 4,
                thinking: false,
                preserve_state: false,
                on_token: None,
            })
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        let now = now_ms();
        let parsed: Vec<[String; 3]> = match serde_json::from_str::<Value>(&resp.text) {
            Ok(v) if v.is_array() => v
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|e| {
                    let arr = e.as_array()?;
                    if arr.len() == 3 {
                        Some([
                            arr[0].as_str()?.to_string(),
                            arr[1].as_str()?.to_string(),
                            arr[2].as_str()?.to_string(),
                        ])
                    } else {
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        };
        let edges = parsed
            .into_iter()
            .map(|[s, p, o]| TemporalEdge {
                subject: s,
                predicate: p,
                object: o,
                valid_from: now,
                valid_to: None,
                source: text.to_string(),
            })
            .collect();
        Ok(edges)
    }

    pub fn merge_triples(&self, edges: Vec<TemporalEdge>) {
        self.graph.lock().unwrap().merge(edges);
    }

    pub fn graph(&self) -> TemporalGraph {
        self.graph.lock().unwrap().clone()
    }

    // ----- Letta tiered memory -----

    pub fn letta(&self, user_id: &str) -> LettaMemory {
        self.letta
            .lock()
            .unwrap()
            .entry(user_id.to_string())
            .or_insert_with(|| LettaMemory::new(self.embedder.as_ref()))
            .clone()
    }

    pub fn letta_core_replace(&self, user_id: &str, old: &str, new: &str) {
        self.letta
            .lock()
            .unwrap()
            .entry(user_id.to_string())
            .or_insert_with(|| LettaMemory::new(self.embedder.as_ref()))
            .core_replace(old, new);
    }

    pub fn letta_core_append(&self, user_id: &str, s: &str) {
        self.letta
            .lock()
            .unwrap()
            .entry(user_id.to_string())
            .or_insert_with(|| LettaMemory::new(self.embedder.as_ref()))
            .core_append(s);
    }

    pub fn letta_recall_append(&self, user_id: &str, s: &str, cap: usize) {
        self.letta
            .lock()
            .unwrap()
            .entry(user_id.to_string())
            .or_insert_with(|| LettaMemory::new(self.embedder.as_ref()))
            .recall_append(s, cap);
    }

    pub fn letta_archival_add(&self, user_id: &str, text: &str) {
        let emb = self.embedder.clone();
        self.letta
            .lock()
            .unwrap()
            .entry(user_id.to_string())
            .or_insert_with(|| LettaMemory::new(emb.as_ref()))
            .archival_add(emb.as_ref(), text);
    }

    pub fn letta_archival_search(&self, user_id: &str, query: &str, k: usize) -> Vec<String> {
        let emb = self.embedder.clone();
        self.letta
            .lock()
            .unwrap()
            .entry(user_id.to_string())
            .or_insert_with(|| LettaMemory::new(emb.as_ref()))
            .archival_search(emb.as_ref(), query, k)
    }
}

fn extract_facts_fallback(text: &str) -> Vec<String> {
    text.split(|c| matches!(c, '.' | '!' | '?' | '\n'))
        .map(|s| s.trim().to_string())
        .filter(|s| s.len() > 3)
        .collect()
}

// ===========================================================================
// Agent tools (drop into ToolRegistry; carry the processor + a backend)
// ===========================================================================

/// `memory_ingest`: run the Mem0 pipeline (extract -> dedup -> resolve) for a
/// user message. Returns the list of resolutions.
pub struct MemoryIngestTool<B: ModelBackend + Send + Sync + 'static> {
    proc: Arc<MemoryProcessor>,
    backend: Arc<B>,
}

impl<B: ModelBackend + Send + Sync + 'static> MemoryIngestTool<B> {
    pub fn new(proc: Arc<MemoryProcessor>, backend: Arc<B>) -> Self {
        Self { proc, backend }
    }
}

#[async_trait]
impl<B: ModelBackend + Send + Sync + 'static> Tool for MemoryIngestTool<B> {
    fn name(&self) -> &str {
        "memory_ingest"
    }
    fn description(&self) -> &str {
        "Ingest a user message: extract durable facts, deduplicate, and resolve conflicts (Mem0). Returns a list of resolutions."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string" },
                "text": { "type": "string", "description": "The user message to remember" }
            },
            "required": ["user_id", "text"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let user_id = input
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput {
                name: "memory_ingest".into(),
                reason: "missing 'user_id'".into(),
            })?;
        let text =
            input
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput {
                    name: "memory_ingest".into(),
                    reason: "missing 'text'".into(),
                })?;
        match self.proc.ingest(self.backend.as_ref(), user_id, text).await {
            Ok(res) => Ok(serde_json::json!({ "resolutions": res })),
            Err(e) => Ok(serde_json::json!({ "ok": false, "error": e.to_string() })),
        }
    }
}

/// `memory_search`: semantic retrieval of remembered facts for a user.
pub struct MemorySearchTool {
    proc: Arc<MemoryProcessor>,
}

impl MemorySearchTool {
    pub fn new(proc: Arc<MemoryProcessor>) -> Self {
        Self { proc }
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn name(&self) -> &str {
        "memory_search"
    }
    fn description(&self) -> &str {
        "Semantically search a user's remembered facts (Mem0 retrieval)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string" },
                "query": { "type": "string" },
                "k": { "type": "number", "description": "Number of facts (default 3)" }
            },
            "required": ["user_id", "query"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let user_id = input
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput {
                name: "memory_search".into(),
                reason: "missing 'user_id'".into(),
            })?;
        let query =
            input
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput {
                    name: "memory_search".into(),
                    reason: "missing 'query'".into(),
                })?;
        let k = input.get("k").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
        let facts = self.proc.retrieve(user_id, query, k);
        Ok(serde_json::json!({ "query": query, "facts": facts }))
    }
}

/// `memory_state`: dialectic user-state synthesis / retrieval (Honcho).
pub struct MemoryStateTool<B: ModelBackend + Send + Sync + 'static> {
    proc: Arc<MemoryProcessor>,
    backend: Arc<B>,
}

impl<B: ModelBackend + Send + Sync + 'static> MemoryStateTool<B> {
    pub fn new(proc: Arc<MemoryProcessor>, backend: Arc<B>) -> Self {
        Self { proc, backend }
    }
}

#[async_trait]
impl<B: ModelBackend + Send + Sync + 'static> Tool for MemoryStateTool<B> {
    fn name(&self) -> &str {
        "memory_state"
    }
    fn description(&self) -> &str {
        "Synthesize/update a user's dynamic state from recent conversation, or return the current state (Honcho)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string" },
                "transcript": { "type": "string", "description": "Recent conversation to reason over" }
            },
            "required": ["user_id"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let user_id = input
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput {
                name: "memory_state".into(),
                reason: "missing 'user_id'".into(),
            })?;
        let transcript = input
            .get("transcript")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match self
            .proc
            .synthesize_state(self.backend.as_ref(), user_id, transcript)
            .await
        {
            Ok(state) => Ok(serde_json::to_value(&state).unwrap_or(Value::Null)),
            Err(e) => Ok(serde_json::json!({ "ok": false, "error": e.to_string() })),
        }
    }
}

/// `memory_graph`: extract + merge temporal knowledge-graph triples (Zep).
pub struct MemoryGraphTool<B: ModelBackend + Send + Sync + 'static> {
    proc: Arc<MemoryProcessor>,
    backend: Arc<B>,
}

impl<B: ModelBackend + Send + Sync + 'static> MemoryGraphTool<B> {
    pub fn new(proc: Arc<MemoryProcessor>, backend: Arc<B>) -> Self {
        Self { proc, backend }
    }
}

#[async_trait]
impl<B: ModelBackend + Send + Sync + 'static> Tool for MemoryGraphTool<B> {
    fn name(&self) -> &str {
        "memory_graph"
    }
    fn description(&self) -> &str {
        "Extract subject-predicate-object triples from text and merge them into the temporal graph (Zep)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "text": { "type": "string", "description": "Text to extract triples from" } },
            "required": ["text"]
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let text =
            input
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput {
                    name: "memory_graph".into(),
                    reason: "missing 'text'".into(),
                })?;
        match self.proc.extract_triples(self.backend.as_ref(), text).await {
            Ok(edges) => {
                let n = edges.len();
                self.proc.merge_triples(edges);
                Ok(serde_json::json!({ "added": n, "graph_size": self.proc.graph().len() }))
            }
            Err(e) => Ok(serde_json::json!({ "ok": false, "error": e.to_string() })),
        }
    }
}

/// Build the four memory tools sharing one processor + backend.
pub fn memory_tools<B: ModelBackend + Send + Sync + 'static>(
    proc: Arc<MemoryProcessor>,
    backend: Arc<B>,
) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(MemoryIngestTool::<B>::new(proc.clone(), backend.clone())) as Arc<dyn Tool>,
        Arc::new(MemorySearchTool::new(proc.clone())),
        Arc::new(MemoryStateTool::<B>::new(proc.clone(), backend.clone())) as Arc<dyn Tool>,
        Arc::new(MemoryGraphTool::<B>::new(proc, backend)) as Arc<dyn Tool>,
    ]
}

#[cfg(test)]
#[path = "tests/memory.rs"]
mod tests;
