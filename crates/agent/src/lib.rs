//! RoCo Agent — autonomous agent orchestration loop.
//!
//! Implements two harnesses under a shared [`BaseAgent`] trait:
//! - [`CommonAgent`](crate::CommonAgent) / [`Agent`] — ReAct-style observe → think → act loop
//! - [`MechanicAgent`](crate::MechanicAgent) / [`MechaAgent`] — code-driven controller + router
//!
//! Both take human input, produce human output, and may use tools (read/write/edit
//! files) in a sandboxed workspace. Built on top of [`roco_engine::ModelBackend`]
//! for inference and [`roco_tools::Tool`] for actions.

pub mod agent_chat;
pub mod base;
pub mod chapter_steering;
pub mod commentary;
pub mod common_agent;
pub mod context;
pub mod error;
pub mod evals;
pub mod interaction;
pub mod mecha_agent;
pub mod memory;
pub mod natural_feedback;
pub mod observability;
pub mod outline_editing;
pub mod plan;
pub mod quality;
pub mod reversibility;
pub mod scheduler;
pub mod sessions;
pub mod story_direction;
pub mod story_engine;
pub mod story_persistence;
pub mod subtask;
pub mod tool_selector;
pub mod util;
pub mod writing_assistant;
// Backward-compat re-export of mecha_agent as mechanistic
#[doc(hidden)]
pub mod mechanistic {
    pub use super::mecha_agent::*;
    /// Backward-compat alias.
    pub type MechanicAgent = MechanisticAgent;
}

pub use agent_chat::AgentChatSession;
pub use base::BaseAgent;
pub use chapter_steering::{ChapterSteerer, GenerationCheckpoint, GenerationState};
pub use commentary::{Commentary, StoryCommentary};
pub use common_agent::{Agent, AgentConfig, AgentStep, AgentTrace, CommonAgent};
pub use context::*;
pub use error::AgentError;
pub use evals::{RevisionGenerator, StoryEval, StoryEvaluator};
pub use interaction::{HumanAction, InteractionMode, InteractionState};
pub use mecha_agent::{MechaAgent, MechanisticAgent};
pub use memory::{MemoryEntry, MemoryStore, RecallTool, RememberTool};
pub use natural_feedback::{Directive, FeedbackIntent, FeedbackParser, ParsedFeedback};
pub use observability::{
    ActionRecord, DecisionRecord, ModelCallRecord, ObservabilitySummary, ObservabilitySystem,
    QualityRecord,
};
pub use outline_editing::{OutlineCommand, OutlineEditResult, OutlineEditor};
pub use plan::{Plan, PlanResult, PlanStep, Planner, StepOutcome};
pub use quality::{QualityAnalyzer, QualityScore, StoryCritique};
pub use reversibility::{ReversibleAction, Snapshot, SnapshotSummary, VersionControl};
/// Re-export of the workspace crate so callers (and the agent example) can
/// build a sandboxed agent via `roco_agent::workspace`.
pub use roco_workspace as workspace;
pub use scheduler::{ScheduleTool, ScheduledOutcome, ScheduledTask, Scheduler};
pub use sessions::{SessionSearchTool, SessionStore, SessionTranscript, SessionTurn};
pub use story_direction::StoryDirection;
pub use story_engine::{PlotState, StoryConfig, StoryEngine};
pub use story_persistence::{StoryPersistence, StoryState, StorySummary};
pub use subtask::{Subtask, SubtaskOutput};
pub use tool_selector::select_relevant;
pub use writing_assistant::{
    CrossReference, DiffAnalysis, WritingAnalysis, WritingAssistant, WritingSuggestion,
};
