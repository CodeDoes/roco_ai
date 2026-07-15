//! Folder-bound, persistent agent sessions (`goals/agent_chat`).
//!
//! An [`AgentChatSession`] ties an agent run to a project *folder*. Everything
//! that should survive across invocations lives under `<folder>/.roco/agent_chat/`:
//!
//! - `memory.json`    — long-term [`MemoryStore`] (facts / preferences)
//! - `sessions.json`  — searchable [`SessionStore`] of past runs
//! - the agent's [`Workspace`] is rooted at `<folder>` itself (so it can
//!   read and edit the project), with the `.roco` metadata dir excluded from
//!   its normal operation only by convention.
//!
//! Because `MemoryStore`/`SessionStore` persist on every write, continuity is
//! automatic: reopen the same folder and the agent remembers and can search
//! its prior runs.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use roco_tools::Tool;
use roco_workspace::{Workspace, WorkspaceKind};

use roco_engine::ModelBackend;

use crate::agent::{Agent, AgentConfig, AgentTrace};
use crate::{MemoryStore, Scheduler, SessionStore};

/// Metadata directory inside a project folder that holds the agent's
/// persisted state.
pub const AGENT_CHAT_DIR: &str = ".roco/agent_chat";

/// A folder-bound agent session: a workspace plus persistent memory and
/// session history, all rooted at a project directory.
pub struct AgentChatSession {
    pub folder: PathBuf,
    pub workspace: Arc<Workspace>,
    pub memory: Arc<MemoryStore>,
    pub sessions: Arc<SessionStore>,
    pub scheduler: Arc<Scheduler>,
}

impl AgentChatSession {
    /// Open (or initialize) a persistent agent session for `folder`.
    ///
    /// Creates `<folder>` and its `.roco/agent_chat` metadata dir if missing,
    /// then loads any existing memory / session history. The workspace is
    /// rooted at `folder` so the agent operates on the project directly.
    pub fn open(folder: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let folder = folder.into();
        std::fs::create_dir_all(&folder)
            .map_err(|e| anyhow::anyhow!("failed to create folder {}: {e}", folder.display()))?;

        let meta = folder.join(AGENT_CHAT_DIR);
        std::fs::create_dir_all(&meta)
            .map_err(|e| anyhow::anyhow!("failed to create {}: {e}", meta.display()))?;

        let memory = Arc::new(MemoryStore::open(meta.join("memory.json"))?);
        let sessions = Arc::new(SessionStore::open(meta.join("sessions.json"))?);
        let scheduler = Arc::new(Scheduler::new());
        let workspace = Arc::new(Workspace::new(folder.clone(), WorkspaceKind::User)?);

        Ok(Self {
            folder,
            workspace,
            memory,
            sessions,
            scheduler,
        })
    }

    /// Build the combined tool set: built-in tools + workspace-scoped tools +
    /// persistent memory + searchable session history + scheduler.
    pub fn build_tools(&self) -> Vec<Arc<dyn Tool>> {
        let mut tools = roco_tools::all_tools();
        tools.extend(Workspace::scoped_tools(self.workspace.clone()));
        tools.extend(MemoryStore::scoped_tools(self.memory.clone()));
        tools.extend(SessionStore::scoped_tools(self.sessions.clone()));
        tools.extend(Scheduler::scoped_tools(self.scheduler.clone()));
        tools
    }

    /// Run a task, then persist the run as a searchable session transcript.
    /// Memory writes persist automatically; the returned [`AgentTrace`] is
    /// also recorded into the session store so future runs can `search_sessions`.
    pub async fn run<B: ModelBackend + Send + Sync>(
        &self,
        backend: &B,
        task: &str,
    ) -> anyhow::Result<AgentTrace> {
        let config = AgentConfig {
            enable_tools: true,
            enable_think: true,
            verbose: false,
            ..Default::default()
        };
        let agent = Agent::with_tools(config, self.build_tools());
        let trace = agent.run(backend, task).await?;

        let id = format!("run-{}", now_secs());
        self.sessions.record_trace(&id, task, &trace);
        Ok(trace)
    }

    /// Explicitly flush persistent state to disk (memory + sessions). Both
    /// already save on write; this is a convenience for callers that want a
    /// deterministic checkpoint.
    pub fn persist(&self) -> anyhow::Result<()> {
        self.memory.save()?;
        self.sessions.save()?;
        Ok(())
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
    use roco_engine::MockBackend;

    fn tmp_folder() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("roco-agent-chat-{stamp:x}"))
    }

    #[tokio::test]
    async fn folder_session_persists_memory_and_sessions_across_reopen() {
        let folder = tmp_folder();
        let chat = AgentChatSession::open(&folder).unwrap();
        // The metadata dir and workspace root exist.
        assert!(folder.join(AGENT_CHAT_DIR).exists());
        assert!(chat.workspace.root() == folder);

        // Seed a long-term preference; it must hit disk.
        chat.memory
            .add("the user prefers Rust for tooling", "preference", vec!["user".into()]);

        // Run a benign task (MockBackend) — records a session transcript.
        let backend = MockBackend::default();
        let trace = chat.run(&backend, "do a benign task").await.unwrap();
        assert!(trace.steps.len() >= 1, "agent should take at least one step");

        let sessions_path = folder.join(AGENT_CHAT_DIR).join("sessions.json");
        let memory_path = folder.join(AGENT_CHAT_DIR).join("memory.json");
        assert!(sessions_path.exists(), "session transcript persisted");
        assert!(memory_path.exists(), "memory persisted");

        // Reopen the same folder — continuity restored.
        drop(chat);
        let chat2 = AgentChatSession::open(&folder).unwrap();
        let recalled = chat2.memory.retrieve("prefers Rust tooling", 5);
        assert!(!recalled.is_empty(), "memory should survive a reopen");
        assert!(!chat2.sessions.is_empty(), "session history should survive a reopen");

        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn build_tools_includes_workspace_and_persistent_tools() {
        let folder = tmp_folder();
        let chat = AgentChatSession::open(&folder).unwrap();
        let names: Vec<String> = chat.build_tools().iter().map(|t| t.name().to_string()).collect();
        assert!(names.contains(&"remember".to_string()));
        assert!(names.contains(&"search_sessions".to_string()));
        assert!(names.contains(&"read".to_string()));
        assert!(names.contains(&"schedule".to_string()));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
