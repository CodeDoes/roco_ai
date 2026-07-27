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

impl AgentError {
    /// Return a human-readable help string suggesting how to fix this error.
    pub fn help(&self) -> Option<&'static str> {
        match self {
            AgentError::BudgetExceeded { .. } => {
                Some("Try: Increase budget_tokens in agent configuration or break tasks into smaller sub-tasks")
            }
            AgentError::StepLimitReached { .. } => {
                Some("Try: Increase max_steps in agent configuration or provide clearer instructions")
            }
            AgentError::ToolNotFound { name } => {
                if name.is_empty() {
                    Some("Try: Ensure tool call XML tags are correctly formatted `<tool_call><name>...</name></tool_call>`")
                } else {
                    Some("Try: Register the tool with the harness tool registry before executing")
                }
            }
            AgentError::ToolError { .. } => {
                Some("Try: Check tool permissions or verify workspace relative paths pass Sandbox check")
            }
            AgentError::BackendError(msg) => {
                if msg.contains("adapter") || msg.contains("GPU") {
                    Some("Try: RWKV_ADAPTER=llvmpipe for CPU fallback")
                } else {
                    Some("Try: Ensure roco-inferd daemon is running or check model weights in models/")
                }
            }
            AgentError::Internal(_) => None,
        }
    }
}

impl std::error::Error for AgentError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_error_help() {
        let err = AgentError::BudgetExceeded { used: 100, max: 50 };
        assert!(err.help().unwrap().contains("budget_tokens"));

        let err2 = AgentError::BackendError("adapter failed".to_string());
        assert!(err2.help().unwrap().contains("llvmpipe"));
    }
}
