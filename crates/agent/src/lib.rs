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

pub use agent::{Agent, AgentConfig, AgentStep, AgentTrace};
pub use error::AgentError;
pub use subtask::{Subtask, SubtaskOutput};
pub use memory::{MemoryEntry, MemoryStore, RememberTool, RecallTool};
pub use plan::{Plan, PlanStep, PlanResult, Planner, StepOutcome};
