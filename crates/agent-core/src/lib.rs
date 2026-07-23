//! RoCo Agent Core — autonomous agent orchestration loop.
//!
//! Implements two harnesses under a shared [`BaseAgent`] trait:
//! - [`CommonAgent`] / [`Agent`] — ReAct-style observe → think → act loop
//! - [`MechaAgent`] / [`MechanisticAgent`] — code-driven controller + router
//!
//! Both take human input, produce human output, and may use tools (read/write/edit
//! files) in a sandboxed workspace. Built on top of [`roco_engine::ModelBackend`]
//! for inference and [`roco_tools::Tool`] for actions.

pub mod agent_chat;
pub mod base;
pub mod common_agent;
pub mod context;
pub mod error;
pub mod interaction;
pub mod mecha_agent;
pub mod memory;
pub mod observability;
pub mod plan;
pub mod reversibility;
pub mod scheduler;
pub mod sessions;
pub mod subtask;
pub mod tool_selector;

pub use agent_chat::AgentChatSession;
pub use base::BaseAgent;
pub use common_agent::{Agent, AgentConfig, AgentStep, AgentTrace, CommonAgent};
pub use context::*;
pub use error::AgentError;
pub use interaction::{HumanAction, InteractionMode, InteractionState};
pub use mecha_agent::{MechaAgent, MechanisticAgent};
pub use memory::{MemoryEntry, MemoryStore, RecallTool, RememberTool};
pub use observability::{
    ActionRecord, DecisionRecord, ModelCallRecord, ObservabilitySummary, ObservabilitySystem,
    QualityRecord,
};
pub use plan::{Plan, PlanResult, PlanStep, Planner, StepOutcome};
pub use reversibility::{ReversibleAction, Snapshot, SnapshotSummary, VersionControl};
/// Re-export of the workspace crate so callers (and the agent example) can
/// build a sandboxed agent via `roco_agent_core::workspace`.
pub use roco_workspace as workspace;
pub use scheduler::{ScheduleTool, ScheduledOutcome, ScheduledTask, Scheduler};
pub use sessions::{SessionSearchTool, SessionStore, SessionTranscript, SessionTurn};
pub use subtask::{Subtask, SubtaskOutput};
pub use tool_selector::select_relevant;

#[cfg(test)]
mod tests;
