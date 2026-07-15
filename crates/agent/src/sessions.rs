//! Persistent session store under `.roco/` combined with content search.
//!
//! Backed by [`roco_session::SessionStore`] for file-based persistence:
//!
//! ```text
//! .roco/sessions/{id}/
//! ├── session.log          ← conversation turns (updated after each message)
//! ├── trace.txt            ← raw I/O transcript, streaming in
//! ├── meta.json            ← parent_id, session_type, active_branch
//! └── history-{branch}.jsonl ← branch checkpoints
//! ```
//!
//! Above that layer sits `SessionTranscript` recording + `SessionSearchTool`
//! so the model can recall past conversations via a `search_sessions` tool.
//! This satisfies `goals/agent/session_search.md`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use roco_session::SessionStore as RoCoSessionStore;
use roco_tools::{Tool, ToolError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One exchanged message within a session transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTurn {
    pub role: String,
    pub text: String,
    pub ts: u64,
}

/// A recorded agent session (one run / one conversation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTranscript {
    pub id: String,
    pub task: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: u64,
    pub turns: Vec<SessionTurn>,
}

impl SessionTranscript {
    /// Build a transcript from an agent run trace.
    pub fn from_trace(id: &str, task: &str, trace: &crate::AgentTrace) -> Self {
        let mut turns = Vec::new();
        for step in &trace.steps {
            if !step.assistant_text.trim().is_empty() {
                turns.push(SessionTurn {
                    role: "assistant".into(),
                    text: step.assistant_text.clone(),
                    ts: crate::memory::now_secs(),
                });
            }
            for (i, tc) in step.tool_calls.iter().enumerate() {
                turns.push(SessionTurn {
                    role: "tool".into(),
                    text: format!("call {}: {}", tc.name, tc.raw),
                    ts: crate::memory::now_secs(),
                });
                if i < step.tool_results.len() {
                    turns.push(SessionTurn {
                        role: "tool_result".into(),
                        text: step.tool_results[i].clone(),
                        ts: crate::memory::now_secs(),
                    });
                }
            }
        }
        Self {
            id: id.to_string(),
            task: task.to_string(),
            tags: Vec::new(),
            created_at: crate::memory::now_secs(),
            turns,
        }
    }

    /// Flatten the transcript into a single searchable string.
    pub fn search_text(&self) -> String {
        let mut s = self.task.clone();
        for t in &self.turns {
            s.push(' ');
            s.push_str(&t.text);
        }
        s
    }
}

/// A persistent store of past sessions, backed by `.roco/` file layout.
///
/// Wraps [`RoCoSessionStore`] for file operations and maintains an in-memory
/// search index populated by [`record`](Self::record) calls.
pub struct SessionStore {
    /// The underlying file-backed session manager (initialized lazily).
    inner: RwLock<Option<RoCoSessionStore>>,
    /// Indexed transcripts for fast content search.
    search_index: RwLock<Vec<SessionTranscript>>,
    /// Base path for `.roco/` root.
    base_path: PathBuf,
    /// Optional JSON file for persisted search index (loaded on init, saved on record).
    index_path: RwLock<Option<PathBuf>>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(".roco")
    }
}

impl SessionStore {
    /// Create a new session store rooted at `base` (defaults to current dir → `.roco`).
    pub fn new<P: AsRef<Path>>(base: P) -> Self {
        Self {
            inner: RwLock::new(None),
            search_index: RwLock::new(Vec::new()),
            base_path: base.as_ref().to_path_buf(),
            index_path: RwLock::new(None),
        }
    }

    /// Initialize the underlying roco_session store and load any existing search index.
    pub fn init(&self) -> anyhow::Result<()> {
        let store = RoCoSessionStore::new(&self.base_path)?;
        *self.inner.write().unwrap() = Some(store);
        // Load persisted search index if it exists
        self.load_search_index()?;
        Ok(())
    }

    /// Set the path where the search index should be persisted.
    pub fn set_index_path<P: AsRef<Path>>(&self, path: P) {
        *self.index_path.write().unwrap() = Some(path.as_ref().to_path_buf());
    }

    /// Open the store at a specific path, loading existing data.
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let base = path.as_ref().to_path_buf();
        let store = Self::new(&base);
        *store.index_path.write().unwrap() = Some(base);
        store.init()?;
        Ok(store)
    }

    /// Create a top-level root session in the file-backed store.
    pub fn create_root(&self, id: &str) -> anyhow::Result<()> {
        let guard = self.inner.read().unwrap();
        let store = guard.as_ref().ok_or_else(|| anyhow::anyhow!("SessionStore not initialized"))?;
        store.create_root(id)?;
        Ok(())
    }

    /// Open an existing session by ID, returning a handle scoped to it.
    pub fn open_session(&self, id: &str) -> anyhow::Result<roco_session::SessionHandle> {
        let guard = self.inner.read().unwrap();
        let store = guard.as_ref().ok_or_else(|| anyhow::anyhow!("SessionStore not initialized"))?;
        Ok(store.open(id)?)
    }

    /// Spawn a sub-session. Logs agent_switch in both traces and records spawn in parent history.
    pub fn spawn_sub<PId: AsRef<str>, SId: AsRef<str>>(
        &self,
        parent_id: PId,
        child_id: SId,
    ) -> anyhow::Result<roco_session::SessionHandle> {
        let guard = self.inner.read().unwrap();
        let store = guard.as_ref().ok_or_else(|| anyhow::anyhow!("SessionStore not initialized"))?;
        Ok(store.spawn_sub(parent_id.as_ref(), child_id.as_ref())?)
    }

    /// Join a child sub-session back into its parent. Records join and marks child finished.
    pub fn join_back<SId: AsRef<str>, Pid: AsRef<str>>(
        &self,
        child_id: SId,
        parent_id: Pid,
        summary: &str,
    ) -> anyhow::Result<()> {
        let guard = self.inner.read().unwrap();
        let store = guard.as_ref().ok_or_else(|| anyhow::anyhow!("SessionStore not initialized"))?;
        store.join_back(child_id.as_ref(), parent_id.as_ref(), summary)?;
        Ok(())
    }

    /// Switch agent context. Logs switch marker in both source and destination traces.
    pub fn switch_agent<SFrom: AsRef<str>, SDest: AsRef<str>>(
        &self,
        from: SFrom,
        dest: SDest,
    ) -> anyhow::Result<()> {
        let guard = self.inner.read().unwrap();
        let store = guard.as_ref().ok_or_else(|| anyhow::anyhow!("SessionStore not initialized"))?;
        store.switch_agent(from.as_ref(), dest.as_ref())?;
        Ok(())
    }

    /// Log a conversation turn to a session's `session.log`.
    pub fn log_conversation<S: AsRef<str>>(&self, session_id: S, text: &str) -> anyhow::Result<()> {
        let guard = self.inner.read().unwrap();
        let store = guard.as_ref().ok_or_else(|| anyhow::anyhow!("SessionStore not initialized"))?;
        store.log_conversation(session_id.as_ref(), text)?;
        Ok(())
    }

    /// Stream a line into a session's `trace.txt` (raw prompt/response).
    pub fn log_trace<S: AsRef<str>>(&self, session_id: S, text: &str) -> anyhow::Result<()> {
        let guard = self.inner.read().unwrap();
        let store = guard.as_ref().ok_or_else(|| anyhow::anyhow!("SessionStore not initialized"))?;
        store.log_trace(session_id.as_ref(), text)?;
        Ok(())
    }

    /// Write an event to the global trace log (`.roco/trace.log`).
    pub fn log_global<E: serde::Serialize>(&self, event: &E) -> anyhow::Result<()> {
        let guard = self.inner.read().unwrap();
        let store = guard.as_ref().ok_or_else(|| anyhow::anyhow!("SessionStore not initialized"))?;
        store.log_global(event)?;
        Ok(())
    }

    /// Record a finished transcript and update the search index.
    pub fn record(&self, transcript: SessionTranscript) {
        let mut idx = self.search_index.write().expect("search index lock poisoned");
        idx.push(transcript);
        let _ = self.save();
    }

    /// Convenience: record an agent run trace as a session.
    pub fn record_trace(&self, id: &str, task: &str, trace: &crate::AgentTrace) {
        self.record(SessionTranscript::from_trace(id, task, trace));
    }

    /// Search past sessions by content, returning the most relevant first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SessionTranscript> {
        let query_tokens = crate::memory::tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }
        let idx = self.search_index.read().expect("search index lock poisoned");
        let mut scored: Vec<(f64, SessionTranscript)> = idx
            .iter()
            .map(|s| {
                (
                    crate::memory::score_text(
                        &query_tokens,
                        &s.search_text(),
                        &s.tags,
                        s.created_at,
                    ),
                    s.clone(),
                )
            })
            .filter(|(sc, _)| *sc > 0.0)
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let limit = if limit == 0 { 5 } else { limit };
        scored.into_iter().take(limit).map(|(_, s)| s).collect()
    }

    pub fn get(&self, id: &str) -> Option<SessionTranscript> {
        let idx = self.search_index.read().expect("search index lock poisoned");
        idx.iter().find(|s| s.id == id).cloned()
    }

    pub fn len(&self) -> usize {
        let idx = self.search_index.read().expect("search index lock poisoned");
        idx.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Persist the search index to disk (if a path was set or via `open()`).
    pub fn save(&self) -> anyhow::Result<()> {
        let idx = self.search_index.read().expect("search index lock poisoned");
        let path = match self.index_path.read().expect("index path lock poisoned").as_ref() {
            Some(p) => p.clone(),
            None => return Ok(()),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&*idx)?;
        std::fs::write(&path, text)?;
        Ok(())
    }

    /// Load previously persisted search index from disk.
    fn load_search_index(&self) -> anyhow::Result<()> {
        let path = match self.index_path.read().expect("index path lock poisoned").as_ref() {
            Some(p) if p.exists() => p.clone(),
            _ => return Ok(()),
        };
        let text = std::fs::read_to_string(&path)?;
        if !text.trim().is_empty() {
            let loaded: Vec<SessionTranscript> = serde_json::from_str(&text)?;
            *self.search_index.write().expect("search index lock poisoned") = loaded;
        }
        Ok(())
    }

    /// The `search_sessions` tool bound to this store.
    pub fn scoped_tools(store: Arc<SessionStore>) -> Vec<Arc<dyn roco_tools::Tool>> {
        vec![Arc::new(SessionSearchTool { store })]
    }
}

/// Tool that searches past sessions by content.
pub struct SessionSearchTool {
    pub(crate) store: Arc<SessionStore>,
}

impl Tool for SessionSearchTool {
    fn name(&self) -> &str {
        "search_sessions"
    }

    fn description(&self) -> &str {
        "Search past agent sessions/conversations by content to recall prior context. \
         Pass `query`; optional `limit` (default 5)."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "What to search past sessions for" },
                "limit": { "type": "integer", "description": "Max results to return" }
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
            .store
            .search(query, limit)
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "task": s.task,
                    "snippet": s.search_text().chars().take(200).collect::<String>(),
                })
            })
            .collect();
        Ok(serde_json::json!({ "results": items, "count": items.len() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentStep, AgentTrace};

    fn transcript(id: &str, task: &str, text: &str) -> SessionTranscript {
        SessionTranscript {
            id: id.into(),
            task: task.into(),
            tags: vec![],
            created_at: crate::memory::now_secs(),
            turns: vec![SessionTurn {
                role: "user".into(),
                text: text.into(),
                ts: 0,
            }],
        }
    }

    #[test]
    fn search_ranks_by_relevance() {
        let store = SessionStore::new();
        store.record(transcript("s1", "rust project", "We built a Rust CLI with clap."));
        store.record(transcript("s2", "france trip", "We planned a trip to Paris, the capital of France."));
        let results = store.search("capital France Paris", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "s2");
    }

    #[test]
    fn search_returns_nothing_for_empty_query() {
        let store = SessionStore::new();
        store.record(transcript("s1", "x", "something memorable about rust"));
        assert!(store.search("a", 5).is_empty());
    }

    #[test]
    fn search_sessions_tool_works() {
        let store = Arc::new(SessionStore::new());
        store.record(transcript("s1", "rust work", "we implemented a rust workspace crate"));
        let tools = SessionStore::scoped_tools(store.clone());
        let tool = tools.iter().find(|t| t.name() == "search_sessions").unwrap();
        let r = tool.call(serde_json::json!({"query": "rust workspace", "limit": 3})).unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["results"][0]["id"], "s1");
    }

    #[test]
    fn record_trace_builds_transcript() {
        let mut trace = AgentTrace::new();
        let mut step = AgentStep::new(1);
        step.assistant_text = "The capital of France is Paris.".into();
        trace.steps.push(step);
        let store = SessionStore::new();
        store.record_trace("run-1", "geography question", &trace);
        assert_eq!(store.len(), 1);
        let results = store.search("capital France", 5);
        assert_eq!(results.len(), 1);
        assert!(results[0].search_text().contains("Paris"));
    }
}
