//! RoCo Agent — autonomous agent orchestration loop.
//!
//! Implements two harnesses under a shared [`BaseAgent`] trait:
//! - [`CommonAgent`](crate::CommonAgent) / [`Agent`] — ReAct-style observe → think → act loop
//! - [`MechanicAgent`](crate::MechanicAgent) / [`MechaAgent`] — code-driven controller + router
//!
//! Both take human input, produce human output, and may use tools (read/write/edit
//! files) in a sandboxed workspace. Built on top of [`roco_engine::ModelBackend`]
//! for inference and [`roco_tools::Tool`] for actions.

pub mod base;
pub mod common_agent;
pub mod mecha_agent;
pub mod error;
pub mod subtask;
pub mod memory;
pub mod plan;
pub mod sessions;
pub mod scheduler;
pub mod tool_selector;
pub mod agent_chat;
// Backward-compat re-export of mecha_agent as mechanistic
#[doc(hidden)]
pub mod mechanistic {
    pub use super::mecha_agent::*;
    /// Backward-compat alias.
    pub type MechanicAgent = MechanisticAgent;
}

pub use base::BaseAgent;
pub use common_agent::{Agent, CommonAgent, AgentConfig, AgentStep, AgentTrace};
pub use mecha_agent::{MechanisticAgent, MechaAgent};
pub use agent_chat::AgentChatSession;
pub use error::AgentError;
pub use subtask::{Subtask, SubtaskOutput};
pub use memory::{MemoryEntry, MemoryStore, RememberTool, RecallTool};
pub use plan::{Plan, PlanStep, PlanResult, Planner, StepOutcome};
pub use sessions::{SessionSearchTool, SessionStore, SessionTranscript, SessionTurn};
pub use scheduler::{ScheduleTool, ScheduledOutcome, ScheduledTask, Scheduler};
pub use tool_selector::select_relevant;
/// Re-export of the workspace crate so callers (and the agent example) can
/// build a sandboxed agent via `roco_agent::workspace`.
pub use roco_workspace as workspace;
