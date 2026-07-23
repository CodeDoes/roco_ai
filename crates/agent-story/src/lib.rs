//! RoCo Agent Story — story generation pipeline.
//!
//! Provides the story engine, quality analysis, outline editing, and related
//! modules for the RoCo collaborative writing system. Depends on
//! [`roco_agent_core`] for interaction types and the core agent framework.

pub mod chapter_steering;
pub mod commentary;
pub mod evals;
pub mod natural_feedback;
pub mod outline_editing;
pub mod quality;
pub mod story_direction;
pub mod story_engine;
pub mod story_persistence;
pub mod util;
pub mod writing_assistant;

pub use chapter_steering::{ChapterSteerer, GenerationCheckpoint, GenerationState};
pub use commentary::{Commentary, StoryCommentary};
pub use evals::{RevisionGenerator, StoryEval, StoryEvaluator};
pub use natural_feedback::{Directive, FeedbackIntent, FeedbackParser, ParsedFeedback};
pub use outline_editing::{OutlineCommand, OutlineEditResult, OutlineEditor};
pub use quality::{QualityAnalyzer, QualityScore, StoryCritique};
pub use story_direction::StoryDirection;
pub use story_engine::{PlotState, StoryConfig, StoryEngine};
pub use story_persistence::{StoryPersistence, StoryState, StorySummary};
pub use writing_assistant::{
    CrossReference, DiffAnalysis, WritingAnalysis, WritingAssistant, WritingSuggestion,
};

#[cfg(test)]
mod tests;
