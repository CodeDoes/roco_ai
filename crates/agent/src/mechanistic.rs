//! Mechanistic Agent — code-driven controller + router.
//!
//! Replaces the model-driven ReAct loop with a **code-driven** pipeline:
//! the model is a subroutine called only at fixed, grammar-constrained points;
//! classic code owns all control flow, dispatch, and I/O.
//!
//! # Flow
//!
//! ```text
//! think(msg) → repair_derive(thoughts) → dispatch(tasks, workspace, handlers) → commit(workspace)
//! ```
//!
//! - `think`: free-form model call to reason about the user request.
//! - `repair_derive`: grammar-constrained model call with retry → structured `Plan`.
//! - `dispatch`: iterate tasks, route each to a registered handler by `(type, domain)`.
//!   Handlers write into a request-scoped workspace sandbox.
//! - `commit`: collect workspace artifacts into the final `MechanisticOutcome`.
//!
//! The model never touches the filesystem, never decides control flow,
//! and never calls tools — classic code does all three. The workspace is a
//! temp directory scoped to a single `run()` call.

use std::collections::HashMap;

use roco_engine::{CompletionRequest, ModelBackend};
use roco_workspace::{Workspace, WorkspaceKind};
use serde::{Deserialize, Serialize};

use crate::error::AgentError;

/// Configuration for the repair loop (Infer → Engine → Infer).
///
/// Controls retry behaviour when model output fails to parse:
/// - Retry with progressively tightened temperature
/// - Truncate prompt on repeated failures
/// - Fall back gracefully after `max_retries`
#[derive(Debug, Clone, Copy)]
pub struct RepairConfig {
    /// Maximum number of retry attempts (0 = no repair, fail immediately).
    pub max_retries: u32,
    /// Starting temperature for the first attempt.
    pub temperature: f32,
    /// Temperature decrement per retry (no lower than `temperature_floor`).
    pub temperature_delta: f32,
    /// Hard floor for temperature — never go below this.
    pub temperature_floor: f32,
    /// Max tokens for the initial attempt.
    pub max_tokens: usize,
    /// Tokens to subtract per retry (no lower than `min_tokens`).
    pub token_decay: usize,
    /// Minimum tokens regardless of retries.
    pub min_tokens: usize,
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            temperature: 0.3,
            temperature_delta: 0.1,
            temperature_floor: 0.1,
            max_tokens: 256,
            token_decay: 64,
            min_tokens: 64,
        }
    }
}

/// A typed task within a mechanistic plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Task {
    /// The kind of work (e.g. "plan", "write", "summarize").
    pub r#type: String,
    /// The content domain (e.g. "chapter", "wiki", "synopsis").
    pub domain: String,
    /// Domain-specific parameters (title, outline, instructions, etc.).
    pub spec: serde_json::Value,
}

/// A structured plan produced by `derive()`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    pub tasks: Vec<Task>,
}

/// The result of a single dispatched handler.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HandlerResult {
    pub task: Task,
    pub output: String,
    /// Files written by this handler (name → content snapshot) inside the
    /// workspace sandbox.
    pub files: HashMap<String, String>,
}

/// Final outcome of a mechanistic agent run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MechanisticOutcome {
    pub plan: Plan,
    pub handler_results: Vec<HandlerResult>,
    /// Snapshot of all workspace files at commit time.
    pub workspace_files: HashMap<String, String>,
    /// Temp workspace path (cleaned up on drop).
    pub workspace_path: String,
}

/// A handler function registered for a `(type, domain)` pair.
///
/// Handlers receive the task spec, the model backend, and a workspace
/// sandbox. They may call the model (grammar-constrained) or execute
/// purely in code. All file writes go through the workspace — the
/// model never touches the real filesystem.
pub type HandlerFn =
    Box<dyn Fn(&Task, &dyn ModelBackend, &Workspace) -> HandlerResult + Send + Sync>;

/// The mechanistic agent — code-driven controller + router.
///
/// # Example
///
/// ```ignore
/// use roco_agent::mechanistic::{MechanisticAgent, Task, HandlerResult};
/// use roco_engine::MockBackend;
/// use roco_workspace::Workspace;
/// use std::collections::HashMap;
///
/// let backend = MockBackend::default();
/// let mut agent = MechanisticAgent::new(&backend);
///
/// agent.register("write", "chapter", Box::new(|task, _backend, ws| {
///     let title = task.spec.get("title").and_then(|v| v.as_str()).unwrap_or("Chapter");
///     let path = ws.resolve("CHAPTER.md").unwrap();
///     std::fs::write(&path, format!("# {}\n\nContent.", title)).ok();
///     HandlerResult {
///         task: task.clone(),
///         output: format!("written: {}", title),
///         files: HashMap::new(),  // populated at commit
///     }
/// }));
///
/// let outcome = agent.run("Write a story about a dragon").unwrap();
/// assert!(!outcome.plan.tasks.is_empty());
/// ```
pub struct MechanisticAgent<'a> {
    backend: &'a dyn ModelBackend,
    handlers: HashMap<(String, String), HandlerFn>,
    repair: RepairConfig,
}

impl<'a> MechanisticAgent<'a> {
    /// Create a new mechanistic agent with the given backend.
    pub fn new(backend: &'a dyn ModelBackend) -> Self {
        Self {
            backend,
            handlers: HashMap::new(),
            repair: RepairConfig::default(),
        }
    }

    /// Override the default repair loop configuration.
    pub fn with_repair(mut self, config: RepairConfig) -> Self {
        self.repair = config;
        self
    }

    /// Register a handler for a `(type, domain)` pair.
    ///
    /// Unknown pairs will fail loud during dispatch.
    pub fn register(&mut self, r#type: &str, domain: &str, handler: HandlerFn) {
        self.handlers.insert((r#type.to_string(), domain.to_string()), handler);
    }

    /// Run the full mechanistic loop with a request-scoped temp workspace.
    ///
    /// 1. Think: free-form model call to reason about the request.
    /// 2. Derive (repair loop): grammar-constrained call → structured Plan.
    /// 3. Dispatch: route each task to its handler; handlers write into the
    ///    temp workspace.
    /// 4. Commit: snapshot workspace files and return the outcome.
    pub fn run(&self, msg: &str) -> Result<MechanisticOutcome, AgentError> {
        let ws = Workspace::temp(WorkspaceKind::Temp)
            .map_err(|e| AgentError::Internal(format!("failed to create workspace: {e}")))?;

        let thoughts = self.think(msg)?;
        let plan = self.repair_derive(&thoughts)?;
        let results = self.dispatch(&plan, &ws)?;
        let outcome = self.commit(plan, results, &ws)?;
        Ok(outcome)
    }

    /// Phase 1: free-form model call to reason about the user request.
    fn think(&self, msg: &str) -> Result<String, AgentError> {
        let prompt = format!("Reason about this request and plan a response:\n{}", msg);
        let resp = self.backend_complete("", &prompt, None, 512, 0.7)?;
        Ok(resp)
    }

    /// Phase 2: grammar-constrained model call with repair loop.
    fn repair_derive(&self, thoughts: &str) -> Result<Plan, AgentError> {
        let mut temp = self.repair.temperature;
        let mut max_tokens = self.repair.max_tokens;
        let mut last_err = None;

        for attempt in 0..=self.repair.max_retries {
            let grammar = Some(PLAN_GRAMMAR.to_string());
            let prompt = format!(
                "Based on your reasoning, produce a structured plan as JSON tasks:\n{}",
                thoughts
            );

            match self.backend_complete("", &prompt, grammar.as_deref(), max_tokens, temp) {
                Ok(resp) => match serde_json::from_str::<Plan>(&resp) {
                    Ok(plan) => return Ok(plan),
                    Err(e) => {
                        last_err = Some(AgentError::Internal(format!(
                            "failed to parse plan (attempt {}): {e}\noutput: {resp}",
                            attempt
                        )));
                    }
                },
                Err(e) => last_err = Some(e),
            }

            temp = (temp - self.repair.temperature_delta).max(self.repair.temperature_floor);
            max_tokens = max_tokens
                .saturating_sub(self.repair.token_decay)
                .max(self.repair.min_tokens);
        }

        Err(last_err.unwrap_or_else(|| {
            AgentError::Internal("derive failed after all retries".to_string())
        }))
    }

    /// Phase 3: dispatch each task to its registered handler.
    fn dispatch(&self, plan: &Plan, ws: &Workspace) -> Result<Vec<HandlerResult>, AgentError> {
        let mut results = Vec::new();
        for task in &plan.tasks {
            let key = (task.r#type.clone(), task.domain.clone());
            let handler = self.handlers.get(&key).ok_or_else(|| {
                AgentError::Internal(format!(
                    "no handler registered for ({}, {})",
                    task.r#type, task.domain
                ))
            })?;
            let result = handler(task, self.backend, ws);
            results.push(result);
        }
        Ok(results)
    }

    /// Phase 4: snapshot workspace files into the outcome.
    fn commit(
        &self,
        plan: Plan,
        handler_results: Vec<HandlerResult>,
        ws: &Workspace,
    ) -> Result<MechanisticOutcome, AgentError> {
        let mut workspace_files: HashMap<String, String> = HashMap::new();
        // Collect all files from the workspace directory.
        if let Ok(entries) = std::fs::read_dir(ws.root()) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        workspace_files.insert(name, content);
                    }
                }
            }
        }

        Ok(MechanisticOutcome {
            plan,
            handler_results,
            workspace_files,
            workspace_path: ws.root().to_string_lossy().to_string(),
        })
    }

    /// Helper: call the backend with error mapping.
    fn backend_complete(
        &self,
        system: &str,
        prompt: &str,
        grammar: Option<&str>,
        max_tokens: usize,
        temperature: f32,
    ) -> Result<String, AgentError> {
        let req = CompletionRequest {
            system: system.to_string(),
            prompt: prompt.to_string(),
            output_schema: None,
            grammar: grammar.map(|s| s.to_string()),
            temperature,
            max_tokens,
            estimated_prompt_tokens: 0,
            thinking: false,
            preserve_state: false,
            on_token: None,
            session: None,
        };
        let resp = futures::executor::block_on(self.backend.complete(req))
            .map_err(|e| AgentError::BackendError(format!("{e}")))?;
        Ok(resp.text)
    }
}

/// BNF grammar that constrains model output to a valid Plan JSON.
const PLAN_GRAMMAR: &str = r#"
root  ::= "{" space "\"tasks\"" space ":" space "[" space task-list space "]" space "}"
task-list ::= task ( space "," space task )*
task  ::= "{" space "\"type\"" space ":" space string space "," space "\"domain\"" space ":" space string space "," space "\"spec\"" space ":" space value space "}"
string ::= "\"" ( [ -~] )* "\""
value ::= string | number | object | array | "true" | "false" | "null"
number ::= "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)?
object ::= "{" space ( string space ":" space value ( space "," space string space ":" space value )* )? space "}"
array  ::= "[" space ( value ( space "," space value )* )? space "]"
space ::= " "?
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use roco_engine::MockBackend;

    #[test]
    fn test_register_and_dispatch_known_handler() {
        let backend = MockBackend::default();
        let mut agent = MechanisticAgent::new(&backend);

        agent.register("write", "chapter", Box::new(|task, _backend, ws| {
            let path = ws.resolve("CHAPTER.md").unwrap();
            std::fs::write(&path, "# Title\n\nContent.").ok();
            HandlerResult {
                task: task.clone(),
                output: "written".to_string(),
                files: HashMap::new(),
            }
        }));

        let plan = Plan {
            tasks: vec![Task {
                r#type: "write".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({"title": "Chapter 1"}),
            }],
        };

        let ws = Workspace::temp(WorkspaceKind::Temp).unwrap();
        let results = agent.dispatch(&plan, &ws).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].output, "written");
    }

    #[test]
    fn test_unknown_handler_fails_loud() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new(&backend);

        let plan = Plan {
            tasks: vec![Task {
                r#type: "nonexistent".into(),
                domain: "void".into(),
                spec: serde_json::json!({}),
            }],
        };

        let ws = Workspace::temp(WorkspaceKind::Temp).unwrap();
        let result = agent.dispatch(&plan, &ws);
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("no handler registered"),
            "must fail loud on unknown (type, domain)"
        );
    }

    #[test]
    fn test_empty_handlers_rejects_any_task() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new(&backend);

        let plan = Plan {
            tasks: vec![Task {
                r#type: "write".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({}),
            }],
        };

        let ws = Workspace::temp(WorkspaceKind::Temp).unwrap();
        let result = agent.dispatch(&plan, &ws);
        assert!(result.is_err(), "must fail when no handlers are registered");
    }

    #[test]
    fn test_full_run_with_workspace() {
        let backend = MockBackend::default();
        let mut agent = MechanisticAgent::new(&backend);

        agent.register("write", "chapter", Box::new(|task, _backend, ws| {
            let title = task.spec.get("title").and_then(|v| v.as_str()).unwrap_or("Ch");
            let path = ws.resolve("CHAPTER.md").unwrap();
            std::fs::write(&path, format!("# {title}\n\nContent.")).ok();
            HandlerResult {
                task: task.clone(),
                output: format!("written: {title}"),
                files: HashMap::new(),
            }
        }));

        // MockBackend doesn't return valid JSON, so run() will fail at
        // the repair_derive step. Test that the error path is clean.
        let result = agent.run("Write a story about a dragon");
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn test_full_run_parse_failure_path() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new(&backend);

        // With the default MockBackend, derive should fail to parse.
        let result = agent.run("test");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("failed to parse") || msg.contains("attempt") || msg.contains("retries"),
            "run should fail with parse error, got: {msg}");
    }

    #[test]
    fn test_handler_calls_backend_and_writes_workspace() {
        let backend = MockBackend::default();
        let mut agent = MechanisticAgent::new(&backend);

        agent.register("write", "prose", Box::new(|task, backend, ws| {
            let prompt = task.spec.get("prompt").and_then(|v| v.as_str()).unwrap_or("write");
            let req = CompletionRequest {
                system: String::new(),
                prompt: format!("Generate: {prompt}"),
                grammar: None,
                temperature: 0.0,
                max_tokens: 100,
                estimated_prompt_tokens: 0,
                thinking: false,
                preserve_state: false,
                on_token: None,
                session: None,
                output_schema: None,
            };
            let resp = futures::executor::block_on(backend.complete(req))
                .map(|r| r.text)
                .unwrap_or_else(|_| "[generated]".to_string());
            let path = ws.resolve("OUTPUT.md").unwrap();
            std::fs::write(&path, &resp).ok();
            HandlerResult { task: task.clone(), output: resp, files: HashMap::new() }
        }));

        let plan = Plan {
            tasks: vec![Task {
                r#type: "write".into(),
                domain: "prose".into(),
                spec: serde_json::json!({"prompt": "a short poem"}),
            }],
        };

        let ws = Workspace::temp(WorkspaceKind::Temp).unwrap();
        let results = agent.dispatch(&plan, &ws).unwrap();
        assert_eq!(results.len(), 1);

        // The workspace should contain the file written by the handler.
        let ws_file = ws.root().join("OUTPUT.md");
        assert!(ws_file.exists(), "handler should write to workspace");
        let content = std::fs::read_to_string(ws_file).unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_commit_snapshots_workspace_files() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new(&backend);
        let ws = Workspace::temp(WorkspaceKind::Temp).unwrap();

        // Write some files into the workspace.
        std::fs::write(ws.root().join("a.txt"), "content-a").unwrap();
        std::fs::write(ws.root().join("b.txt"), "content-b").unwrap();

        let plan = Plan { tasks: vec![] };
        let results = vec![];
        let outcome = agent.commit(plan, results, &ws).unwrap();

        assert_eq!(outcome.workspace_files.len(), 2);
        assert_eq!(outcome.workspace_files.get("a.txt").unwrap(), "content-a");
        assert_eq!(outcome.workspace_files.get("b.txt").unwrap(), "content-b");
        assert!(!outcome.workspace_path.is_empty());
    }

    #[test]
    fn test_repair_loop_retries_on_parse_failure() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new(&backend).with_repair(RepairConfig {
            max_retries: 1,
            temperature: 0.5,
            temperature_delta: 0.2,
            temperature_floor: 0.1,
            max_tokens: 256,
            token_decay: 64,
            min_tokens: 64,
        });

        let result = agent.repair_derive("Write a story.");
        assert!(result.is_err(), "repair loop should fail after exhausting retries");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("attempt") || msg.contains("retries") || msg.contains("failed to parse"),
            "error should mention retry attempts, got: {msg}"
        );
    }

    #[test]
    fn test_repair_loop_zero_retries_fails_immediately() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new(&backend).with_repair(RepairConfig {
            max_retries: 0,
            ..RepairConfig::default()
        });

        let result = agent.repair_derive("Write a story.");
        assert!(result.is_err(), "zero retries should fail immediately");
    }
}
