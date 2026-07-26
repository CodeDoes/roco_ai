use serde_json::Value;

/// A callable tool that the agent can invoke.
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    fn call(&self, args: Value) -> Result<Value, ToolError>;
}

#[derive(Debug, Clone)]
pub struct ToolError(pub String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tool error: {}", self.0)
    }
}

impl std::error::Error for ToolError {}
