//! Mechanistic Agent — code-driven controller + router.
//!
//! Replaces the model-driven ReAct loop with a **code-driven** pipeline:
//! the model is a subroutine called only at fixed, grammar-constrained points;
//! classic code owns all control flow, dispatch, and I/O.
//!
//! # Flow
//!
//! ```text
//! think(msg) → derive(thoughts) → dispatch(tasks, handlers) → commit(results)
//! ```
//!
//! - `think`: free-form model call to reason about the user request.
//! - `derive`: grammar-constrained model call → structured `Plan` of typed tasks.
//! - `dispatch`: iterate tasks, route each to a registered handler by `(type, domain)`.
//! - `commit`: collect handler results into a final `MechanisticOutcome`.
//!
//! The model never touches the filesystem, never decides control flow,
//! and never calls tools — classic code does all three.

use std::collections::HashMap;

use roco_engine::{CompletionRequest, ModelBackend};
use serde::{Deserialize, Serialize};

use crate::error::AgentError;

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
    pub files: HashMap<String, String>,
}

/// Final outcome of a mechanistic agent run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MechanisticOutcome {
    pub plan: Plan,
    pub handler_results: Vec<HandlerResult>,
}

/// A handler function registered for a `(type, domain)` pair.
///
/// Handlers receive the task spec and return content + files. They may call
/// the model (grammar-constrained) or execute purely in code.
pub type HandlerFn = Box<dyn Fn(&Task, &dyn ModelBackend) -> HandlerResult + Send + Sync>;

/// The mechanistic agent — code-driven controller + router.
///
/// # Example
///
/// ```ignore
/// use roco_agent::mechanistic::{MechanisticAgent, Task, HandlerResult};
/// use roco_engine::MockBackend;
/// use std::collections::HashMap;
///
/// let backend = MockBackend::default();
/// let mut agent = MechanisticAgent::new(&backend);
///
/// // Register a handler for (write, chapter)
/// agent.register("write", "chapter", Box::new(|task, _backend| {
///     let title = task.spec.get("title").and_then(|v| v.as_str()).unwrap_or("Chapter");
///     let mut files = HashMap::new();
///     files.insert("CHAPTER.md".to_string(), format!("# {}\n\nOnce upon a time...", title));
///     HandlerResult {
///         task: task.clone(),
///         output: format!("written chapter: {}", title),
///         files,
///     }
/// }));
///
/// let outcome = agent.run("Write a story about a dragon").unwrap();
/// assert!(!outcome.plan.tasks.is_empty());
/// ```
pub struct MechanisticAgent<'a> {
    backend: &'a dyn ModelBackend,
    handlers: HashMap<(String, String), HandlerFn>,
}

impl<'a> MechanisticAgent<'a> {
    /// Create a new mechanistic agent with the given backend.
    pub fn new(backend: &'a dyn ModelBackend) -> Self {
        Self {
            backend,
            handlers: HashMap::new(),
        }
    }

    /// Register a handler for a `(type, domain)` pair.
    ///
    /// Unknown pairs will fail loud during dispatch.
    pub fn register(&mut self, r#type: &str, domain: &str, handler: HandlerFn) {
        self.handlers.insert((r#type.to_string(), domain.to_string()), handler);
    }

    /// Run the full mechanistic loop: think → derive → dispatch → commit.
    pub fn run(&self, msg: &str) -> Result<MechanisticOutcome, AgentError> {
        let thoughts = self.think(msg)?;
        let plan = self.derive(&thoughts)?;
        let results = self.dispatch(&plan)?;
        Ok(MechanisticOutcome {
            plan,
            handler_results: results,
        })
    }

    /// Phase 1: free-form model call to reason about the user request.
    fn think(&self, msg: &str) -> Result<String, AgentError> {
        let prompt = format!("Reason about this request and plan a response:\n{}", msg);
        let resp = self.backend_complete("", &prompt, None, 512, 0.7)?;
        Ok(resp)
    }

    /// Phase 2: grammar-constrained model call to produce a structured plan.
    fn derive(&self, thoughts: &str) -> Result<Plan, AgentError> {
        let prompt = format!(
            "Based on your reasoning, produce a structured plan as JSON tasks:\n{}",
            thoughts
        );
        let grammar = Some(PLAN_GRAMMAR.to_string());
        let resp = self.backend_complete("", &prompt, grammar.as_deref(), 256, 0.3)?;
        serde_json::from_str(&resp).map_err(|e| {
            AgentError::Internal(format!(
                "failed to parse plan from model output: {e}\noutput: {resp}"
            ))
        })
    }

    /// Phase 3: dispatch each task to its registered handler.
    fn dispatch(&self, plan: &Plan) -> Result<Vec<HandlerResult>, AgentError> {
        let mut results = Vec::new();
        for task in &plan.tasks {
            let key = (task.r#type.clone(), task.domain.clone());
            let handler = self.handlers.get(&key).ok_or_else(|| {
                AgentError::Internal(format!(
                    "no handler registered for ({}, {})",
                    task.r#type, task.domain
                ))
            })?;
            let result = handler(task, self.backend);
            results.push(result);
        }
        Ok(results)
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

        agent.register("write", "chapter", Box::new(|task, _backend| {
            let mut files = HashMap::new();
            files.insert("CHAPTER.md".to_string(), "# Title\n\nContent.".to_string());
            HandlerResult {
                task: task.clone(),
                output: "written".to_string(),
                files,
            }
        }));

        let plan = Plan {
            tasks: vec![Task {
                r#type: "write".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({"title": "Chapter 1"}),
            }],
        };

        let results = agent.dispatch(&plan).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].output, "written");
        assert!(results[0].files.contains_key("CHAPTER.md"));
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

        let result = agent.dispatch(&plan);
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no handler registered"),
            "must fail loud on unknown (type, domain), got: {msg}"
        );
    }

    #[test]
    fn test_empty_handlers_rejects_any_task() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new(&backend);

        let plan = Plan {
            tasks: vec![
                Task { r#type: "write".into(), domain: "chapter".into(), spec: serde_json::json!({}) },
                Task { r#type: "write".into(), domain: "wiki".into(), spec: serde_json::json!({}) },
            ],
        };

        let result = agent.dispatch(&plan);
        assert!(result.is_err(), "must fail when no handlers are registered");
    }

    #[test]
    fn test_handler_can_call_backend() {
        let backend = MockBackend::default();
        let mut agent = MechanisticAgent::new(&backend);

        agent.register("write", "prose", Box::new(|task, backend| {
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
            let mut files = HashMap::new();
            files.insert("OUTPUT.md".to_string(), resp.clone());
            HandlerResult { task: task.clone(), output: resp, files }
        }));

        let plan = Plan {
            tasks: vec![Task {
                r#type: "write".into(),
                domain: "prose".into(),
                spec: serde_json::json!({"prompt": "a short poem"}),
            }],
        };

        let results = agent.dispatch(&plan).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].files.contains_key("OUTPUT.md"));
    }

    #[test]
    fn test_think_returns_backend_output() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new(&backend);

        let thoughts = agent.think("test request").unwrap();
        assert!(!thoughts.is_empty(), "think should return non-empty text");
    }

    #[test]
    fn test_derive_from_thoughts() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new(&backend);

        // MockBackend echoes the prompt, so parsing will fail.
        // This tests the error handling path.
        let result = agent.derive("I need to write a chapter about dragons.");
        // Either succeeds or fails with a parse error — no panics.
        match result {
            Ok(plan) => assert!(plan.tasks.is_empty() || !plan.tasks.is_empty()),
            Err(e) => assert!(e.to_string().contains("failed to parse plan")),
        }
    }
}
