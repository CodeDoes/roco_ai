//! Persistent session store — saves/loads chat sessions as JSON files under `.roco/sessions/`.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::Message;

/// A saved session with full message history + metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub id: String,
    pub objective: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub turn_count: usize,
    pub messages: Vec<Message>,
}

/// Lightweight summary for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub objective: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub turn_count: usize,
    pub message_count: usize,
}

/// File-based session store under a root directory.
#[derive(Debug, Clone)]
pub struct SessionStore {
    dir: PathBuf,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(".roco/sessions")
    }
}

impl SessionStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let _ = fs::create_dir_all(&dir);
        Self { dir }
    }

    /// Generate a unique session ID from the current timestamp.
    pub fn generate_id() -> String {
        format!(
            "s{:.6}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0)
        )
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", id))
    }

    /// Save a session. Creates the directory if needed.
    pub fn save(&self, session: &SessionData) -> Result<()> {
        let _ = fs::create_dir_all(&self.dir);
        let path = self.path_for(&session.id);
        let json = serde_json::to_string_pretty(session)
            .context("serializing session")?;
        fs::write(&path, json)
            .with_context(|| format!("writing session {}", path.display()))?;
        Ok(())
    }

    /// Load a session by ID.
    pub fn load(&self, id: &str) -> Result<SessionData> {
        let path = self.path_for(id);
        let json = fs::read_to_string(&path)
            .with_context(|| format!("reading session {}", path.display()))?;
        let session: SessionData = serde_json::from_str(&json)
            .with_context(|| format!("parsing session {}", path.display()))?;
        Ok(session)
    }

    /// List all saved sessions, newest first.
    pub fn list(&self) -> Result<Vec<SessionSummary>> {
        let _ = fs::create_dir_all(&self.dir);
        let mut sessions: Vec<SessionSummary> = Vec::new();

        for entry in fs::read_dir(&self.dir).context("reading sessions dir")? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(json) = fs::read_to_string(&path) {
                if let Ok(session) = serde_json::from_str::<SessionData>(&json) {
                    sessions.push(SessionSummary {
                        message_count: session.messages.len(),
                        ..SessionSummary::from(&session)
                    });
                }
            }
        }

        // Sort newest first
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Find the most recently updated session.
    pub fn latest(&self) -> Result<SessionData> {
        let list = self.list()?;
        list.first()
            .and_then(|s| self.load(&s.id).ok())
            .ok_or_else(|| anyhow::anyhow!("no sessions found"))
    }

    /// Delete a session.
    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.path_for(id);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("deleting session {}", path.display()))?;
        }
        Ok(())
    }
}

// --- Conversions ---

impl SessionSummary {
    pub fn from(session: &SessionData) -> Self {
        Self {
            id: session.id.clone(),
            objective: session.objective.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            turn_count: session.turn_count,
            message_count: session.messages.len(),
        }
    }
}

impl SessionData {
    /// Create a new SessionData from an objective string.
    pub fn new(objective: impl Into<String>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let id = SessionStore::generate_id();
        Self {
            id,
            objective: objective.into(),
            created_at: now,
            updated_at: now,
            turn_count: 0,
            messages: Vec::new(),
        }
    }

    /// Build from an existing message list (used when resuming).
    pub fn from_messages(objective: impl Into<String>, messages: Vec<Message>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let turn_count = messages.iter().filter(|m| m.role == "user").count();
        Self {
            id: SessionStore::generate_id(),
            objective: objective.into(),
            created_at: now,
            updated_at: now,
            turn_count,
            messages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_session() {
        let dir = std::env::temp_dir().join("roco-session-test");
        let _ = fs::remove_dir_all(&dir);

        let store = SessionStore::new(&dir);
        let mut session = SessionData::new("test session");
        session.messages.push(Message {
            role: "user".into(),
            content: "Hello".into(),
        });
        store.save(&session).unwrap();

        let loaded = store.load(&session.id).unwrap();
        assert_eq!(loaded.objective, "test session");
        assert_eq!(loaded.messages.len(), 1);

        let list = store.list().unwrap();
        assert!(!list.is_empty());
        assert_eq!(list[0].id, session.id);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_returns_most_recent() {
        let dir = std::env::temp_dir().join("roco-session-latest-test");
        let _ = fs::remove_dir_all(&dir);

        let store = SessionStore::new(&dir);
        let s1 = SessionData::new("first");
        let mut s2 = SessionData::new("second");
        std::thread::sleep(std::time::Duration::from_millis(2));
        s2.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        store.save(&s1).unwrap();
        store.save(&s2).unwrap();

        let latest = store.latest().unwrap();
        assert_eq!(latest.objective, "second");

        let _ = fs::remove_dir_all(&dir);
    }
}
