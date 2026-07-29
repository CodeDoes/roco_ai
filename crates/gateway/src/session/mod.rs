//! Session management for the gateway.
//!
//! A Session is a persistent conversation context that survives client
//! disconnections. The gateway owns session lifecycle, not inferd.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Session lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session created but no bake yet.
    Idle,
    /// Currently generating tokens.
    Generating,
    /// Generation completed successfully.
    Completed,
    /// Generation failed or was cancelled.
    Error,
    /// Session archived (no longer active).
    Archived,
}

/// A single session managed by the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub workspace_id: String,
    pub status: SessionStatus,
    pub system_prompt: String,
    pub baked_shots: usize,
    pub created_at: u64,
    pub last_accessed_at: u64,
    pub accumulated_tokens: Vec<String>,
    pub error_message: Option<String>,
}

impl Session {
    pub fn new(id: impl Into<String>, workspace_id: impl Into<String>) -> Self {
        let now = now_secs();
        Self {
            id: id.into(),
            workspace_id: workspace_id.into(),
            status: SessionStatus::Idle,
            system_prompt: String::new(),
            baked_shots: 0,
            created_at: now,
            last_accessed_at: now,
            accumulated_tokens: Vec::new(),
            error_message: None,
        }
    }

    pub fn touch(&mut self) {
        self.last_accessed_at = now_secs();
    }

    /// Switch this session to a different workspace.
    pub fn set_workspace(&mut self, workspace_id: impl Into<String>) {
        self.workspace_id = workspace_id.into();
        self.touch();
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, SessionStatus::Idle | SessionStatus::Generating)
    }
}

/// Manages all sessions in the gateway.
pub struct SessionManager {
    sessions: RwLock<HashMap<String, Session>>,
    /// Max sessions before LRU eviction.
    capacity: usize,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            capacity: 1000,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            capacity,
        }
    }

    /// Create a new session. Returns the session ID.
    pub fn create(&self, workspace_id: impl Into<String>) -> String {
        let id = format!("sess-{}", uuid::Uuid::new_v4());
        let session = Session::new(&id, workspace_id);
        self.sessions.write().insert(id.clone(), session);
        info!("Created session {}", id);
        id
    }

    /// Get a session by ID.
    pub fn get(&self, id: &str) -> Option<Session> {
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get_mut(id) {
            session.touch();
            Some(session.clone())
        } else {
            None
        }
    }

    /// Update a session.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut Session)) -> bool {
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get_mut(id) {
            f(session);
            session.touch();
            true
        } else {
            false
        }
    }

    /// Set session status.
    pub fn set_status(&self, id: &str, status: SessionStatus) -> bool {
        self.update(id, |s| s.status = status)
    }

    /// Append tokens to a session's accumulated output.
    pub fn append_tokens(&self, id: &str, tokens: Vec<String>) -> bool {
        self.update(id, |s| s.accumulated_tokens.extend(tokens))
    }

    /// Get accumulated tokens for a session.
    pub fn get_tokens(&self, id: &str) -> Option<Vec<String>> {
        self.get(id).map(|s| s.accumulated_tokens.clone())
    }

    /// Archive a session (soft delete).
    pub fn archive(&self, id: &str) -> bool {
        self.update(id, |s| s.status = SessionStatus::Archived)
    }

    /// Delete a session permanently.
    pub fn delete(&self, id: &str) -> bool {
        let mut sessions = self.sessions.write();
        sessions.remove(id).is_some()
    }

    /// List all active sessions for a workspace.
    pub fn list_for_workspace(&self, workspace_id: &str) -> Vec<Session> {
        let sessions = self.sessions.read();
        sessions
            .values()
            .filter(|s| s.workspace_id == workspace_id && s.is_active())
            .cloned()
            .collect()
    }

    /// List all sessions (for admin/debug).
    pub fn list_all(&self) -> Vec<Session> {
        let sessions = self.sessions.read();
        sessions.values().cloned().collect()
    }

    /// Evict oldest archived sessions if over capacity.
    pub fn maybe_evict(&self) {
        let mut sessions = self.sessions.write();
        if sessions.len() <= self.capacity {
            return;
        }
        let mut entries: Vec<(String, u64)> = sessions
            .iter()
            .filter(|(_, s)| !s.is_active())
            .map(|(id, s)| (id.clone(), s.last_accessed_at))
            .collect();
        entries.sort_by_key(|(_, t)| *t);
        let to_remove = sessions.len() - self.capacity;
        for (id, _) in entries.into_iter().take(to_remove) {
            sessions.remove(&id);
            info!("Evicted session {}", id);
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_lifecycle() {
        let mgr = SessionManager::new();
        let id = mgr.create("ws-1");
        assert!(mgr.get(&id).is_some());
        assert_eq!(mgr.get(&id).unwrap().status, SessionStatus::Idle);

        mgr.set_status(&id, SessionStatus::Generating);
        assert_eq!(mgr.get(&id).unwrap().status, SessionStatus::Generating);

        mgr.append_tokens(&id, vec!["Hello".into(), " world".into()]);
        assert_eq!(mgr.get_tokens(&id).unwrap().len(), 2);

        mgr.archive(&id);
        assert_eq!(mgr.get(&id).unwrap().status, SessionStatus::Archived);
    }

    #[test]
    fn session_switch_workspace() {
        let mgr = SessionManager::new();
        let id = mgr.create("ws-1");
        assert_eq!(mgr.get(&id).unwrap().workspace_id, "ws-1");

        mgr.update(&id, |s| s.set_workspace("ws-2"));
        assert_eq!(mgr.get(&id).unwrap().workspace_id, "ws-2");
    }
}
