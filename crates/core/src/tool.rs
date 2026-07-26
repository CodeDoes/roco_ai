//! The Tool trait — moved from `crates/tools` into core.
//!
//! All tool implementations (workspace tools, memory tools, scheduling tools)
//! implement this trait. The trait is in core so `crates/agent` can define
//! its own tools without depending on `crates/tools`.

use serde_json::Value;

/// A tool that the model can call.
pub trait Tool: Send + Sync {
    /// Tool name (used in the model's tool-calling protocol).
    fn name(&self) -> &str;

    /// Human-readable description for the model.
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's parameters.
    fn schema(&self) -> Value;

    /// Execute the tool with the given arguments.
    fn call(&self, args: Value) -> Result<Value, ToolError>;
}

/// Error from a tool invocation.
#[derive(Debug, Clone)]
pub struct ToolError(pub String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ToolError {}

impl From<String> for ToolError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ToolError {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}


