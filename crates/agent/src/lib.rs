//! RoCo Agent — autonomous agent orchestration loop.
//!
//! Implements the agent loop: plan → execute subtasks → evaluate → iterate.
//! Built on top of [`roco_engine::ModelBackend`] for inference and
//! [`roco_tools::Tool`] for actions.

pub mod agent;
pub mod error;
pub mod subtask;
pub mod memory;
pub mod plan;
pub mod sessions;
pub mod scheduler;
pub mod tool_selector;
pub mod agent_chat;
pub mod mechanistic;

pub use agent::{Agent, AgentConfig, AgentStep, AgentTrace};
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
