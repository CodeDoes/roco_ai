use std::fmt;

#[derive(Debug)]
pub enum AgentError {
    BudgetExceeded { used: usize, max: usize },
    StepLimitReached { used: u32, max: u32 },
    ToolNotFound { name: String },
    ToolError { name: String, message: String },
    BackendError(String),
    Internal(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::BudgetExceeded { used, max } => {
                write!(f, "budget exceeded: used {used} of {max} tokens")
            }
            AgentError::StepLimitReached { used, max } => {
                write!(f, "step limit reached: {used} of {max} iterations")
            }
            AgentError::ToolNotFound { name } => write!(f, "tool not found: {name}"),
            AgentError::ToolError { name, message } => {
                write!(f, "tool `{name}` error: {message}")
            }
            AgentError::BackendError(msg) => write!(f, "backend error: {msg}"),
            AgentError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for AgentError {}
