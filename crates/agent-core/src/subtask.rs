//! Subtask types for the agent planning/execution system.

use roco_engine::TokenUsage;
use roco_tools::ToolCall;

/// A unit of work within the agent's plan.
#[derive(Debug, Clone)]
pub struct Subtask {
    pub id: String,
    pub objective: String,
    pub context: String,
    pub max_tokens: usize,
    pub temperature: f32,
}

/// The output from executing a subtask.
#[derive(Debug, Clone)]
pub struct SubtaskOutput {
    pub subtask_id: String,
    pub text: String,
    pub usage: TokenUsage,
    pub success: bool,
    /// Any tool calls that were emitted during the subtask.
    pub tool_calls: Vec<ToolCall>,
}

impl SubtaskOutput {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            subtask_id: id.into(),
            text: String::new(),
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
            },
            success: false,
            tool_calls: Vec::new(),
        }
    }
}
