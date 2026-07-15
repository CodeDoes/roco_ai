/// Session metadata persisted to `meta.json`.

use serde::{Deserialize, Serialize};

/// Whether this session is a top-level root or spawned by another session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionType {
    Root,
    Sub,
}

/// Current lifecycle state of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Session is actively executing.
    Active,
    /// Sub-sessions were spawned and have since joined back.
    Joined,
    /// All work complete.
    Finished,
}

/// Persistent metadata for a single session.
/// Written to `sessions/{id}/meta.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub parent_id: Option<String>,
    pub session_type: SessionType,
    pub status: SessionStatus,
    /// Which history branch is currently active (if any).
    pub active_branch: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl SessionMeta {
    pub fn new_root(id: impl Into<String>) -> Self {
        let ts = current_ts();
        Self {
            session_id: id.into(),
            parent_id: None,
            session_type: SessionType::Root,
            status: SessionStatus::Active,
            active_branch: None,
            created_at: ts,
            updated_at: ts,
        }
    }

    pub fn sub(id: impl Into<String>, parent_id: impl Into<String>) -> Self {
        let ts = current_ts();
        Self {
            session_id: id.into(),
            parent_id: Some(parent_id.into()),
            session_type: SessionType::Sub,
            status: SessionStatus::Active,
            active_branch: None,
            created_at: ts,
            updated_at: ts,
        }
    }

    pub fn updated(&mut self) {
        self.updated_at = current_ts();
    }

    pub fn finish(&mut self) {
        self.status = SessionStatus::Finished;
        self.updated();
    }
}

/// A recorded branch event within a session.
/// Written to `sessions/{id}/history-{branch_id}.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub ts: u64,
    #[serde(rename = "type")]
    pub kind: HistoryKind,
    pub child_session: Option<String>,
    pub merge_info: Option<String>,
}

/// Kind of branch event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HistoryKind {
    Spawned,
    Joined,
}

/// An event written to the global trace log.
/// Written to `.roco/trace.log` as one line per call / decision.
#[derive(Debug, Clone, Serialize)]
pub struct GlobalTraceEvent<'a> {
    pub ts: u64,
    pub kind: &'static str,
    pub session: &'a str,
    /// Optional parent session for sub-agents.
    pub parent: Option<&'a str>,
    /// Brief description.
    pub detail: &'a str,
}

fn current_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
