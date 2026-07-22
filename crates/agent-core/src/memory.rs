//! Long-term agent memory.
//!
//! Provides a [`MemoryStore`] that persists facts / notes / preferences so
//! the agent can recall them across sessions. Memory is exposed to the model
//! as two [`roco_tools::Tool`]s — `remember` and `recall` — so an agent run
//! can read and write its own long-term context. This satisfies
//! `goals/agent/memory.md`.
//!
//! Retrieval is a deterministic, dependency-free ranking over keyword overlap
//! (with substring tolerance) plus a mild recency bonus — no external
//! embeddings required, which keeps it runnable on the local RWKV stack.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use roco_tools::{Tool, ToolError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single stored memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub text: String,
    /// Category: `fact`, `note`, `preference`, etc.
    pub kind: String,
    pub tags: Vec<String>,
    pub created_at: u64,
    pub accessed_at: u64,
}

impl MemoryEntry {
    fn touch(&mut self) {
        self.accessed_at = now_secs();
    }
}

static MEM_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn new_id() -> String {
    let c = MEM_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("mem-{}-{:x}", now_secs(), c)
}

pub(crate) fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_lowercase())
        .filter(|t| t.len() >= 3)
        .collect()
}

/// A persistent, in-process memory store.
///
/// Internally uses an `RwLock` so the `remember` / `recall` tools (which hold
/// a shared `Arc<MemoryStore>`) can read and write concurrently. If a `path`
/// is set (via [`MemoryStore::open`]), each write is persisted to disk as a
/// JSON array.
#[derive(Debug)]
pub struct MemoryStore {
    entries: RwLock<Vec<MemoryEntry>>,
    path: RwLock<Option<PathBuf>>,
    capacity: usize,
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            path: RwLock::new(None),
            capacity: 1000,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            path: RwLock::new(None),
            capacity: capacity.max(1),
        }
    }

    /// Open a persistent store at `path`, loading existing entries if present.
    pub fn open(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let store = Self::new();
        if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            if !text.trim().is_empty() {
                let loaded: Vec<MemoryEntry> = serde_json::from_str(&text)?;
                *store.entries.write().expect("mem lock poisoned") = loaded;
            }
        }
        *store.path.write().expect("mem path lock poisoned") = Some(path);
        Ok(store)
    }

    /// Store a new memory. Returns the new entry's id.
    pub fn add(&self, text: &str, kind: &str, tags: Vec<String>) -> String {
        let id = new_id();
        let entry = MemoryEntry {
            id: id.clone(),
            text: text.to_string(),
            kind: if kind.is_empty() {
                "note".to_string()
            } else {
                kind.to_string()
            },
            tags,
            created_at: now_secs(),
            accessed_at: now_secs(),
        };
        {
            let mut entries = self.entries.write().expect("mem lock poisoned");
            entries.push(entry);
            if entries.len() > self.capacity {
                entries.sort_by_key(|e| e.accessed_at);
                let excess = entries.len() - self.capacity;
                entries.drain(0..excess);
            }
        }
        let _ = self.save();
        id
    }

    /// Retrieve the most relevant memories for `query` (up to `limit`).
    pub fn retrieve(&self, query: &str, limit: usize) -> Vec<MemoryEntry> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }
        let entries = self.entries.read().expect("mem lock poisoned");
        let mut scored: Vec<(f64, MemoryEntry)> = entries
            .iter()
            .map(|e| {
                (
                    score_text(&query_tokens, &e.text, &e.tags, e.created_at),
                    e.clone(),
                )
            })
            .filter(|(s, _)| *s > 0.0)
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let limit = if limit == 0 { 5 } else { limit };
        let out: Vec<MemoryEntry> = scored.into_iter().take(limit).map(|(_, e)| e).collect();
        drop(entries);

        // Mark returned entries as accessed (recency for future ranking).
        let mut entries = self.entries.write().expect("mem lock poisoned");
        for o in &out {
            if let Some(e) = entries.iter_mut().find(|x| x.id == o.id) {
                e.touch();
            }
        }
        out
    }

    /// Convenience: just the recalled texts.
    pub fn retrieve_texts(&self, query: &str, limit: usize) -> Vec<String> {
        self.retrieve(query, limit)
            .into_iter()
            .map(|e| e.text)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.read().expect("mem lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Persist the store to its `path`, if one is configured.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = self.path.read().expect("mem path lock poisoned");
        if let Some(p) = path.as_ref() {
            let entries = self.entries.read().expect("mem lock poisoned");
            let text = serde_json::to_string_pretty(&*entries)?;
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(p, text)?;
        }
        Ok(())
    }

    /// Remove all entries (and overwrite the persisted file, if any).
    pub fn clear(&self) {
        self.entries.write().expect("mem lock poisoned").clear();
        let _ = self.save();
    }

    /// The two memory tools (`remember`, `recall`) bound to this store.
    pub fn scoped_tools(mem: Arc<MemoryStore>) -> Vec<Arc<dyn Tool>> {
        vec![
            Arc::new(RememberTool { mem: mem.clone() }),
            Arc::new(RecallTool { mem }),
        ]
    }
}

/// Shared relevance scorer used by both [`MemoryStore`] and
/// [`crate::sessions::SessionStore`]. Returns 0.0 when there is no overlap
/// (so the entry is filtered out).
pub(crate) fn score_text(
    query_tokens: &[String],
    text: &str,
    tags: &[String],
    created_at: u64,
) -> f64 {
    let mut hay: Vec<String> = tokenize(text);
    for t in tags {
        hay.extend(tokenize(t));
    }
    let mut hits = 0u32;
    for qt in query_tokens {
        if hay
            .iter()
            .any(|ht| ht == qt || ht.contains(qt) || qt.contains(ht))
        {
            hits += 1;
        }
    }
    if hits == 0 {
        return 0.0;
    }
    let recall = hits as f64 / query_tokens.len() as f64;
    let age = now_secs().saturating_sub(created_at);
    let recency = 1.0 / (1.0 + age as f64 / 86_400.0);
    recall * (0.7 + 0.3 * recency)
}

// ── RememberTool ────────────────────────────────────────────────

pub struct RememberTool {
    pub(crate) mem: Arc<MemoryStore>,
}

impl Tool for RememberTool {
    fn name(&self) -> &str {
        "remember"
    }
    fn description(&self) -> &str {
        "Store a fact, note, or preference in long-term memory so it persists across sessions. \
         Pass `text`; optional `kind` (fact|note|preference) and `tags` (keywords)."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "The content to remember"},
                "kind": {"type": "string", "description": "Category: fact, note, or preference"},
                "tags": {"type": "array", "items": {"type": "string"}, "description": "Optional keywords to aid recall"}
            },
            "required": ["text"]
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError("missing 'text' argument".into()))?;
        let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("note");
        let tags = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let id = self.mem.add(text, kind, tags);
        Ok(serde_json::json!({ "ok": true, "id": id, "remembered": text }))
    }
}

// ── RecallTool ──────────────────────────────────────────────────

pub struct RecallTool {
    pub(crate) mem: Arc<MemoryStore>,
}

impl Tool for RecallTool {
    fn name(&self) -> &str {
        "recall"
    }
    fn description(&self) -> &str {
        "Search long-term memory for relevant entries. Pass `query`; optional `limit` (default 5)."
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "What to recall"},
                "limit": {"type": "integer", "description": "Max results to return"}
            },
            "required": ["query"]
        })
    }
    fn call(&self, args: Value) -> Result<Value, ToolError> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError("missing 'query' argument".into()))?;
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
        let items: Vec<Value> = self
            .mem
            .retrieve(query, limit)
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "text": e.text,
                    "kind": e.kind,
                    "tags": e.tags
                })
            })
            .collect();
        Ok(serde_json::json!({ "results": items, "count": items.len() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_then_recall_returns_entry() {
        let mem = MemoryStore::new();
        mem.add(
            "The capital of France is Paris.",
            "fact",
            vec!["geography".into()],
        );
        let results = mem.retrieve("capital of France", 5);
        assert_eq!(results.len(), 1);
        assert!(results[0].text.contains("Paris"));
    }

    #[test]
    fn recall_ranks_by_relevance() {
        let mem = MemoryStore::new();
        mem.add(
            "Rust uses ownership for memory safety.",
            "fact",
            vec!["rust".into()],
        );
        mem.add(
            "Paris is the capital of France and lies on the Seine.",
            "fact",
            vec!["geography".into()],
        );
        let results = mem.retrieve("capital France Paris", 5);
        assert!(!results.is_empty());
        assert!(
            results[0].text.contains("Paris"),
            "most relevant should rank first"
        );
    }

    #[test]
    fn recall_returns_nothing_for_empty_query() {
        let mem = MemoryStore::new();
        mem.add("something memorable", "note", vec![]);
        assert!(mem.retrieve("a", 5).is_empty());
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = std::env::temp_dir().join(format!("roco-mem-test-{}.json", now_secs()));
        {
            let mem = MemoryStore::open(&dir).unwrap();
            mem.add(
                "persistent fact about the user",
                "preference",
                vec!["user".into()],
            );
            mem.save().unwrap();
        }
        {
            let mem = MemoryStore::open(&dir).unwrap();
            let results = mem.retrieve("persistent user preference", 5);
            assert_eq!(results.len(), 1);
            assert!(results[0].text.contains("persistent"));
        }
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn capacity_eviction_keeps_within_bound() {
        let mem = MemoryStore::with_capacity(3);
        for i in 0..10 {
            mem.add(&format!("entry number {i}"), "note", vec![]);
        }
        assert!(mem.len() <= 3, "should not exceed capacity");
    }

    #[test]
    fn remember_and_recall_tools_work() {
        let mem = Arc::new(MemoryStore::new());
        let tools = MemoryStore::scoped_tools(mem.clone());
        let remember = tools.iter().find(|t| t.name() == "remember").unwrap();
        let r = remember
            .call(serde_json::json!({"text": "user prefers dark mode", "kind": "preference", "tags": ["ui"]}))
            .unwrap();
        assert_eq!(r["ok"], true);

        let recall = tools.iter().find(|t| t.name() == "recall").unwrap();
        let r = recall
            .call(serde_json::json!({"query": "user ui preference"}))
            .unwrap();
        assert_eq!(r["count"], 1);
        assert!(r["results"][0]["text"]
            .as_str()
            .unwrap()
            .contains("dark mode"));
    }
}
