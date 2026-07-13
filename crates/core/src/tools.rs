//! Tool registry and function-calling scaffolding for RoCo AI agents.
//!
//! Tools let a Worker call external functions (calculators, file IO, HTTP,
//! code execution, ...). Each tool publishes a JSON Schema describing its
//! inputs, which is embedded in the model prompt so a backend can emit
//! tool-call JSON; the registry validates that JSON and dispatches it.
//!
//! This is backend-agnostic: it works identically with [`MockBackend`] and any
//! real [`ModelBackend`](crate::engine::ModelBackend). See `src/agent.rs` for
//! how a `ToolRegistry` is intended to be handed to a Worker.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    Unknown(String),
    #[error("tool '{name}' input validation failed: {reason}")]
    InvalidInput { name: String, reason: String },
    #[error("tool '{name}' execution failed: {detail}")]
    Execution { name: String, detail: String },
}

/// A callable agent tool.
///
/// Implementors describe their inputs with a JSON Schema object so the schema
/// can be injected into the prompt and the returned tool-call JSON can be
/// validated before [`Tool::run`] is invoked.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Stable identifier used in tool-call JSON and dispatch.
    fn name(&self) -> &str;
    /// One-line description of what the tool does (shown to the model).
    fn description(&self) -> &str;
    /// JSON Schema (object) describing the tool's input.
    fn input_schema(&self) -> Value;
    /// Execute the tool against validated JSON input.
    async fn run(&self, input: Value) -> Result<Value, ToolError>;
}

/// Registry of available tools, keyed by name.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool. Re-registering a name replaces the previous tool.
    pub fn register(&mut self, tool: Arc<dyn Tool>) -> &mut Self {
        self.tools.insert(tool.name().to_string(), tool);
        self
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// All registered tools (handy for grammar/prompt generation).
    pub fn all_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    /// All tool descriptors as a JSON array, ready to embed in a prompt.
    pub fn schemas_json(&self) -> Value {
        let mut arr = Vec::with_capacity(self.tools.len());
        for t in self.tools.values() {
            arr.push(serde_json::json!({
                "name": t.name(),
                "description": t.description(),
                "input_schema": t.input_schema(),
            }));
        }
        Value::Array(arr)
    }

    /// Return a new `ToolRegistry` containing only the tools whose names
    /// match the given predicate.  Useful for gradual disclosure — the agent
    /// can expose a curated subset of tools to the model rather than dumping
    /// the entire catalogue at once.
    pub fn filter<F>(&self, predicate: F) -> Self
    where
        F: Fn(&str) -> bool,
    {
        let mut filtered = ToolRegistry::new();
        for (name, tool) in &self.tools {
            if predicate(name) {
                filtered.tools.insert(name.clone(), Arc::clone(tool));
            }
        }
        filtered
    }

    /// Validate `input` against the tool's schema (lightweight structural check).
    pub fn validate_input(&self, name: &str, input: &Value) -> Result<(), ToolError> {
        let tool = self
            .get(name)
            .ok_or_else(|| ToolError::Unknown(name.to_string()))?;
        validate_against_schema(input, &tool.input_schema()).map_err(|reason| {
            ToolError::InvalidInput {
                name: name.to_string(),
                reason,
            }
        })
    }

    /// Validate then run a tool, returning its JSON output.
    pub async fn dispatch(&self, name: &str, input: Value) -> Result<Value, ToolError> {
        self.validate_input(name, &input)?;
        // Safe: validate_input above guarantees the tool exists.
        let tool = self.get(name).unwrap();
        tool.run(input).await.map_err(|e| match e {
            ToolError::Execution { name, detail } => ToolError::Execution { name, detail },
            other => other,
        })
    }
}

/// Lightweight structural validation against a subset of JSON Schema:
/// - when `schema.type == "object"`, the input must be a JSON object;
/// - every entry of `schema.required` must be present.
///
/// Full Draft-07 validation (types, enums, nesting) is intentionally out of
/// scope; this catches the common malformed-tool-call cases.
fn validate_against_schema(input: &Value, schema: &Value) -> Result<(), String> {
    if schema.get("type").and_then(|v| v.as_str()) == Some("object") {
        let obj = input
            .as_object()
            .ok_or_else(|| "input must be a JSON object".to_string())?;
        if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
            for req in required {
                if let Some(key) = req.as_str() {
                    if !obj.contains_key(key) {
                        return Err(format!("missing required property '{key}'"));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Echoes its `message` input. Useful as a smoke tool and for tests.
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "Echo the provided message back to the caller."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "message": { "type": "string" } },
            "required": ["message"],
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let msg = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput {
                name: "echo".into(),
                reason: "missing 'message'".into(),
            })?;
        Ok(serde_json::json!({ "echo": msg }))
    }
}

/// Sums an array of numbers. Demonstrates typed numeric handling.
pub struct AddTool;

#[async_trait]
impl Tool for AddTool {
    fn name(&self) -> &str {
        "add"
    }
    fn description(&self) -> &str {
        "Return the sum of an array of numbers."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": { "numbers": { "type": "array", "items": { "type": "number" } } },
            "required": ["numbers"],
        })
    }
    async fn run(&self, input: Value) -> Result<Value, ToolError> {
        let nums = input
            .get("numbers")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::InvalidInput {
                name: "add".into(),
                reason: "missing 'numbers'".into(),
            })?;
        let mut sum: f64 = 0.0;
        for n in nums {
            sum += n.as_f64().ok_or_else(|| ToolError::InvalidInput {
                name: "add".into(),
                reason: "non-numeric entry in 'numbers'".into(),
            })?;
        }
        Ok(serde_json::json!({ "sum": sum }))
    }
}

/// Lists all registered tools with their names, descriptions, and
/// input-schema summaries.  The model can call this to discover what
/// tools are available at runtime.
pub struct ListToolsTool {
    /// Reference to the registry so this tool can enumerate the others.
    registry: ToolRegistry,
}

impl ListToolsTool {
    pub fn new(registry: ToolRegistry) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for ListToolsTool {
    fn name(&self) -> &str {
        "list_tools"
    }
    fn description(&self) -> &str {
        "List all available tools with their names, descriptions, and input schemas."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "verbose": {
                    "type": "boolean",
                    "description": "If true, include full input schemas"
                }
            }
        })
    }
    async fn run(&self, _input: Value) -> Result<Value, ToolError> {
        let verbose = _input
            .get("verbose")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let tools: Vec<Value> = self
            .registry
            .all_tools()
            .iter()
            .map(|t| {
                if verbose {
                    serde_json::json!({
                        "name": t.name(),
                        "description": t.description(),
                        "input_schema": t.input_schema(),
                    })
                } else {
                    serde_json::json!({
                        "name": t.name(),
                        "description": t.description(),
                    })
                }
            })
            .collect();
        Ok(serde_json::json!({"tools": tools}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registry() -> ToolRegistry {
        let mut r = ToolRegistry::new();
        r.register(Arc::new(EchoTool));
        r.register(Arc::new(AddTool));
        r
    }

    fn sample_registry_with_list() -> ToolRegistry {
        let mut r = ToolRegistry::new();
        r.register(Arc::new(EchoTool));
        r.register(Arc::new(AddTool));
        // Clone the registry first (without list_tools), so ListToolsTool
        // can enumerate all tools including itself.
        let reg_for_list = r.clone();
        r.register(Arc::new(ListToolsTool::new(reg_for_list)));
        r
    }

    #[test]
    fn registry_lists_and_exposes_schemas() {
        let r = sample_registry();
        assert_eq!(r.len(), 2);
        assert!(r.contains("echo"));
        assert!(!r.contains("nope"));
        let schemas = r.schemas_json();
        let arr = schemas.as_array().expect("schemas_json is an array");
        assert_eq!(arr.len(), 2);
        assert!(arr.iter().any(|s| s["name"] == "echo"));
        assert!(arr.iter().any(|s| s["name"] == "add"));
    }

    #[tokio::test]
    async fn echo_tool_round_trips() {
        let r = sample_registry();
        let out = r
            .dispatch("echo", serde_json::json!({ "message": "hi" }))
            .await
            .unwrap();
        assert_eq!(out["echo"], "hi");
    }

    #[tokio::test]
    async fn add_tool_sums_numbers() {
        let r = sample_registry();
        let out = r
            .dispatch("add", serde_json::json!({ "numbers": [1, 2, 3.5] }))
            .await
            .unwrap();
        assert_eq!(out["sum"], 6.5);
    }

    #[tokio::test]
    async fn list_tools_tool_lists_all_tools() {
        let r = sample_registry_with_list();
        let out = r
            .dispatch("list_tools", serde_json::json!({}))
            .await
            .unwrap();
        let tools = out["tools"].as_array().expect("tools should be an array");
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"echo"), "should include echo: {names:?}");
        assert!(names.contains(&"add"), "should include add: {names:?}");
        // list_tools captures the registry at construction time (before it's
        // registered), so it does *not* appear in its own output.
        // Non-verbose: descriptions present, no input_schema.
        for t in tools.iter() {
            assert!(
                t["description"].as_str().is_some(),
                "tool should have description: {t}"
            );
            assert!(
                t.get("input_schema").is_none(),
                "non-verbose should omit schema: {t}"
            );
        }

        // Verbose mode includes input_schema.
        let out_v = r
            .dispatch("list_tools", serde_json::json!({"verbose": true}))
            .await
            .unwrap();
        let tools_v = out_v["tools"].as_array().unwrap();
        assert!(
            tools_v[0].get("input_schema").is_some(),
            "verbose should include schema"
        );
    }

    #[tokio::test]
    async fn dispatch_unknown_tool_errors() {
        let r = sample_registry();
        let err = r
            .dispatch("ghost", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Unknown(_)));
    }

    #[tokio::test]
    async fn missing_required_field_is_rejected() {
        let r = sample_registry();
        let err = r.dispatch("echo", serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn wrong_input_shape_is_rejected() {
        let r = sample_registry();
        // input must be an object, not an array
        let err = r
            .dispatch("add", serde_json::json!([1, 2, 3]))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput { .. }));
    }

    #[test]
    fn filter_reduces_to_matching_tools() {
        let r = sample_registry();
        let filtered = r.filter(|name| name == "echo");
        assert_eq!(filtered.len(), 1);
        assert!(filtered.contains("echo"));
        assert!(!filtered.contains("add"));
        // Original is unmodified.
        assert_eq!(r.len(), 2);
    }
}
