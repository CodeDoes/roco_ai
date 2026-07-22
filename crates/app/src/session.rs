//! Session capabilities: `session`, `session_agent`, `session_agent_message`.
//!
//! Wraps `roco_session::store::SessionStore` and `roco_agent::SessionStore`
//! so surfaces don't import either directly.

use std::path::PathBuf;
use std::sync::Arc;

use roco_agent::SessionStore as AgentSessionStore;
use roco_session::store::SessionStore as CoreSessionStore;

use crate::{AppError, AppResult};

/// Handle to an open conversation session. Surfaces hold this and call
/// `session_agent` / `session_agent_message` against it.
#[derive(Clone)]
pub struct SessionHandle {
    pub id: String,
    core: Arc<CoreSessionStore>,
    agent_store: Arc<AgentSessionStore>,
}

impl SessionHandle {
    /// Open (creating if missing) a session under `root`.
    pub fn open(root: &PathBuf, id: &str) -> AppResult<Self> {
        let core =
            Arc::new(CoreSessionStore::new(root).map_err(|e| AppError::Session(e.0.clone()))?);
        let _ = core.create_root(id);
        let agent_store = Arc::new(AgentSessionStore::new(root.join("agent")));
        Ok(Self {
            id: id.to_string(),
            core,
            agent_store,
        })
    }

    /// List all session ids under `root`.
    pub fn list(root: &PathBuf) -> Vec<String> {
        let mut ids = Vec::new();
        // Sessions live under `<root>/.roco/sessions/`.
        let sessions_dir = root.join(".roco").join("sessions");
        if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                        ids.push(name.to_string());
                    }
                }
            }
        }
        ids.sort();
        ids
    }

    /// Append a conversation turn (the `session_agent_message` op).
    pub fn message(&self, role: &str, text: &str) -> AppResult<()> {
        self.core
            .log_conversation(self.id.clone(), &format!("[{role}] {text}"))
            .map_err(|e| AppError::Session(e.0.clone()))
    }

    /// Persist the agent transcript for this session.
    pub fn record_transcript(&self, transcript: roco_agent::SessionTranscript) {
        self.agent_store.record(transcript);
    }
}

/// An agent persona bound to a session. Surfaces use this to send messages
/// "as" the agent and to switch personas.
pub struct SessionAgent {
    pub agent: String,
    session: Arc<SessionHandle>,
}

impl SessionAgent {
    /// Bind `agent` to `session`.
    pub fn bind(session: &Arc<SessionHandle>, agent: &str) -> AppResult<Self> {
        session
            .core
            .switch_agent(session.id.clone(), agent.to_string())
            .map_err(|e| AppError::Session(e.0.clone()))?;
        Ok(Self {
            agent: agent.to_string(),
            session: Arc::clone(session),
        })
    }

    /// Log a message from this agent into the session.
    pub fn message(&self, text: &str) -> AppResult<()> {
        self.session.message(&self.agent, text)
    }

    /// Switch to a different agent persona on the same session.
    pub fn switch_to(&mut self, agent: &str) -> AppResult<()> {
        self.session
            .core
            .switch_agent(self.session.id.clone(), agent.to_string())
            .map_err(|e| AppError::Session(e.0.clone()))?;
        self.agent = agent.to_string();
        Ok(())
    }
}
