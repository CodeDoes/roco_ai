//! Parse and execute tool calls from a model response, under the sandbox +
//! policy safety layer.
//!
//! This is the glue that connects the four foundation modules:
//!  - [`crate::grammar`] defines the `<tool_call>` GBNF a model is constrained to;
//!  - [`crate::tools`] holds the [`ToolRegistry`] of callable tools;
//!  - [`crate::sandbox`] executes `bash`/shell tools safely;
//!  - [`crate::policy`] gates every action before it runs.
//!
//! The orchestrator calls [`execute_tool_calls`] on a worker's raw output to
//! turn model-emitted calls into real results.

use serde_json::Value;

use crate::policy::{Action, Policy, PolicyVerdict};
use crate::sandbox::Sandbox;
use crate::tools::ToolRegistry;

/// A parsed `<tool_call>` block.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

/// Outcome of executing a single tool call.
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub call: ToolCall,
    pub verdict: PolicyVerdict,
    /// Present when the action was `Allow`ed and executed successfully.
    pub output: Option<Value>,
    /// Present when execution errored (still `Allow`ed, but the tool failed).
    pub error: Option<String>,
}

/// Tools whose `arguments.command` carries a shell command to run via the sandbox.
const SHELL_TOOLS: &[&str] = &["bash", "shell", "run", "sh"];

/// Extract all `<tool_call>` blocks from a response string.
///
/// Tolerates the whitespace the GBNF grammar emits (tabs/newlines around the
/// JSON) because the inner text is parsed as JSON.
pub fn parse_tool_calls(response: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut rest = response;
    while let (Some(start), Some(end)) = (rest.find("<tool_call>"), rest.find("</tool_call>")) {
        if start >= end {
            break;
        }
        let inner = &rest[start + "<tool_call>".len()..end];
        if let Ok(v) = serde_json::from_str::<Value>(inner.trim()) {
            let name = v
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = v
                .get("arguments")
                .or_else(|| v.get("args"))
                .cloned()
                .unwrap_or(Value::Null);
            if !name.is_empty() {
                calls.push(ToolCall { name, arguments });
            }
        }
        rest = &rest[end + "</tool_call>".len()..];
    }
    calls
}

impl ToolCall {
    /// Map this call to the [`Action`] the policy will vet.
    fn action(&self) -> Action {
        if SHELL_TOOLS.contains(&self.name.as_str()) {
            let command = self
                .arguments
                .get("command")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            Action::RunCommand { command }
        } else {
            Action::CallTool {
                name: self.name.clone(),
                input: self.arguments.to_string(),
            }
        }
    }
}

/// Parse and execute every tool call in `response`: vet each via `policy`, then
/// dispatch through `registry` (regular tools) or `sandbox` (shell tools).
/// Denied / review-pending calls are not executed.
pub async fn execute_tool_calls(
    response: &str,
    registry: &ToolRegistry,
    sandbox: &Sandbox,
    policy: &dyn Policy,
) -> Vec<ToolExecutionResult> {
    let mut results = Vec::new();
    for call in parse_tool_calls(response) {
        let verdict = policy.evaluate(&call.action());
        let mut result = ToolExecutionResult {
            call: call.clone(),
            verdict: verdict.clone(),
            output: None,
            error: None,
        };
        if let PolicyVerdict::Allow = verdict {
            let executed = execute_allowed(&call, registry, sandbox).await;
            if let Some(err) = executed.get("error").and_then(|e| e.as_str()) {
                result.error = Some(err.to_string());
            } else {
                result.output = Some(executed);
            }
        }
        results.push(result);
    }
    results
}

async fn execute_allowed(call: &ToolCall, registry: &ToolRegistry, sandbox: &Sandbox) -> Value {
    if SHELL_TOOLS.contains(&call.name.as_str()) {
        let command = call
            .arguments
            .get("command")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        match sandbox.run_shell(&command) {
            Ok(out) => serde_json::json!({
                "stdout": out.stdout,
                "stderr": out.stderr,
                "exit_code": out.exit_code,
                "timed_out": out.timed_out,
            }),
            Err(e) => serde_json::json!({ "error": e.to_string() }),
        }
    } else {
        match registry.dispatch(&call.name, call.arguments.clone()).await {
            Ok(v) => v,
            Err(e) => serde_json::json!({ "error": e.to_string() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{ComposedPolicy, SandboxGuardPolicy};
    use crate::sandbox::GuardPolicy;
    use crate::tools::{AddTool, EchoTool, ToolRegistry};

    fn registry() -> ToolRegistry {
        let mut r = ToolRegistry::new();
        r.register(std::sync::Arc::new(EchoTool));
        r.register(std::sync::Arc::new(AddTool));
        r
    }

    fn response_with(calls: &[&str]) -> String {
        calls
            .iter()
            .map(|c| format!("\t<tool_call>\n\t{}\n\t</tool_call>", c))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn parses_multiple_tool_calls() {
        let resp = response_with(&[
            r#"{"name":"echo","arguments":{"message":"hi"}}"#,
            r#"{"name":"add","arguments":{"numbers":[1,2,3]}}"#,
        ]);
        let calls = parse_tool_calls(&resp);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "echo");
        assert_eq!(calls[1].name, "add");
    }

    #[tokio::test]
    async fn allows_and_dispatches_tool() {
        let resp = response_with(&[r#"{"name":"add","arguments":{"numbers":[1,2,3]}}"#]);
        let results = execute_tool_calls(&resp, &registry(), &Sandbox::new(), &ComposedPolicy::new()).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verdict, PolicyVerdict::Allow);
        assert_eq!(results[0].output.as_ref().unwrap()["sum"], 6.0);
    }

    #[tokio::test]
    async fn sandbox_guard_blocks_dangerous_shell_call() {
        let policy = SandboxGuardPolicy::new(GuardPolicy::DenyList(vec!["rm -rf /".to_string()]));
        let resp = response_with(&[r#"{"name":"bash","arguments":{"command":"rm -rf /"}}"#]);
        let results = execute_tool_calls(&resp, &registry(), &Sandbox::new(), &policy).await;
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0].verdict, PolicyVerdict::Deny(_)));
        assert!(results[0].output.is_none(), "denied call must not execute");
    }

    #[tokio::test]
    async fn allows_safe_shell_call_through_sandbox() {
        let policy = SandboxGuardPolicy::new(GuardPolicy::DenyList(vec!["rm -rf /".to_string()]));
        let resp = response_with(&[r#"{"name":"bash","arguments":{"command":"echo carried-on"}}"#]);
        let results = execute_tool_calls(&resp, &registry(), &Sandbox::new(), &policy).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verdict, PolicyVerdict::Allow);
        let out = results[0].output.as_ref().unwrap();
        assert!(out["stdout"].as_str().unwrap().contains("carried-on"));
    }
}
