//! Session search — recall prior conversations by content.
//!
//! A [`SessionStore`] records agent runs as [`SessionTranscript`]s and lets the
//! model search them by content via a `search_sessions` tool. Retrieval reuses
//! the same keyword+recency ranking as [`crate::memory`] (`score_text`), so
//! "What did we decide about X last week?" becomes a content search over past
//! transcripts. This satisfies `goals/agent/session_search.md`.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

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

/// A persistent store of past sessions, searchable by content.
pub struct SessionStore {
    sessions: RwLock<Vec<SessionTranscript>>,
    path: RwLock<Option<PathBuf>>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(Vec::new()),
            path: RwLock::new(None),
        }
    }

    /// Open a persistent store at `path`, loading existing transcripts if present.
    pub fn open(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let store = Self::new();
        if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            if !text.trim().is_empty() {
                let loaded: Vec<SessionTranscript> = serde_json::from_str(&text)?;
                *store.sessions.write().expect("sess lock poisoned") = loaded;
            }
        }
        *store.path.write().expect("sess path lock poisoned") = Some(path);
        Ok(store)
    }

    /// Record a finished transcript.
    pub fn record(&self, transcript: SessionTranscript) {
        self.sessions.write().expect("sess lock poisoned").push(transcript);
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
        let sessions = self.sessions.read().expect("sess lock poisoned");
        let mut scored: Vec<(f64, SessionTranscript)> = sessions
            .iter()
            .map(|s| {
                (
                    crate::memory::score_text(&query_tokens, &s.search_text(), &s.tags, s.created_at),
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
        self.sessions
            .read()
            .expect("sess lock poisoned")
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.sessions.read().expect("sess lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Persist the store to its `path`, if one is configured.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = self.path.read().expect("sess path lock poisoned");
        if let Some(p) = path.as_ref() {
            let sessions = self.sessions.read().expect("sess lock poisoned");
            let text = serde_json::to_string_pretty(&*sessions)?;
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(p, text)?;
        }
        Ok(())
    }

    /// The `search_sessions` tool bound to this store.
    pub fn scoped_tools(store: Arc<SessionStore>) -> Vec<Arc<dyn Tool>> {
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
                "query": {"type": "string", "description": "What to search past sessions for"},
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
    fn persistence_roundtrip() {
        let dir = std::env::temp_dir().join(format!("roco-sess-test-{}.json", crate::memory::now_secs()));
        {
            let store = SessionStore::open(&dir).unwrap();
            store.record(transcript("s1", "persisted session", "remember this decision about the API"));
            store.save().unwrap();
        }
        {
            let store = SessionStore::open(&dir).unwrap();
            let results = store.search("persisted decision API", 5);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].id, "s1");
        }
        let _ = std::fs::remove_file(&dir);
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
