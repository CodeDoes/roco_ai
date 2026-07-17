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
pub mod context;
pub mod error;
pub mod subtask;
pub mod memory;
pub mod plan;
pub mod sessions;
pub mod scheduler;
pub mod tool_selector;
pub mod agent_chat;
pub mod story_engine;
pub mod quality;
pub mod evals;
pub mod story_persistence;
pub mod observability;
pub mod reversibility;
pub mod commentary;
pub mod writing_assistant;
pub mod interaction;
pub mod natural_feedback;
pub mod outline_editing;
pub mod story_direction;
pub mod chapter_steering;
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
pub use story_engine::{StoryEngine, StoryConfig, PlotState};
pub use quality::{QualityAnalyzer, QualityScore, StoryCritique};
pub use evals::{StoryEvaluator, StoryEval, RevisionGenerator};
pub use story_persistence::{StoryPersistence, StoryState, StorySummary};
pub use observability::{ObservabilitySystem, ObservabilitySummary, ModelCallRecord, DecisionRecord, ActionRecord, QualityRecord};
pub use reversibility::{VersionControl, Snapshot, ReversibleAction, SnapshotSummary};
pub use commentary::{Commentary, StoryCommentary};
pub use writing_assistant::{WritingAssistant, WritingAnalysis, WritingSuggestion, DiffAnalysis, CrossReference};
pub use interaction::{InteractionMode, InteractionState, HumanAction};
pub use natural_feedback::{FeedbackParser, ParsedFeedback, FeedbackIntent, Directive};
pub use outline_editing::{OutlineEditor, OutlineCommand, OutlineEditResult};
pub use story_direction::StoryDirection;
pub use chapter_steering::{ChapterSteerer, GenerationState, GenerationCheckpoint};
pub use context::*;
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
