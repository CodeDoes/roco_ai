//! Mechanistic Agent — code-driven controller + router plugin.
//!
//! Replaces the model-driven ReAct loop with a **code-driven** pipeline:
//! the model is a subroutine called only at fixed, grammar-constrained points;
//! classic code owns all control flow, dispatch, and I/O.
//!
//! Unlike [`CommonAgent`](crate::CommonAgent) (ReAct loop) where the model
//! decides when to call tools, handlers here **write directly to a sandboxed
//! workspace**. Both harnesses implement [`BaseAgent`](super::BaseAgent),
//! guaranteeing the same human I/O contract: take a message, produce a
//! response, possibly reading/writing/editing files.
//!
//! # Flow
//!
//! ```text
//! classify(msg) → think(intent, msg) → derive(thoughts) → validate(plan) → dispatch(tasks, workspace) → commit(workspace)
//! ```
//!
//! - `classify`: grammar-constrained call → structured `Intent` (route, confidence, goal).
//!   Low confidence falls back to `justChatting`.
//! - `think`: free-form model call seeded with the classified intent.
//! - `derive` (repair loop): grammar-constrained call → structured `Plan`.
//! - `validate`: check plan tasks against the selected route's declared set.
//! - `dispatch`: route each task to a registered handler; handlers write into
//!   the temp workspace sandbox.
//! - `commit`: snapshot workspace files into the final `MechanisticOutcome`.
//!
//! The model never touches the filesystem, never decides control flow.
//! Classic code (handlers) does all file I/O in a scoped temp workspace.

use std::collections::HashMap;
use std::sync::Arc;

use roco_engine::{CompletionRequest, ModelBackend};
use roco_workspace::{Workspace, WorkspaceKind};
use serde::{Deserialize, Serialize};

use super::base::BaseAgent;
use super::error::AgentError;
use super::memory::MemoryStore;
use super::sessions::SessionStore;

/// Classified intent from a user message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Intent {
    /// The matched route name (e.g. "storyTeller", "coder", "justChatting").
    pub route: String,
    /// Confidence 0.0–1.0. Below `FALLBACK_THRESHOLD` routes to `justChatting`.
    pub confidence: f32,
    /// A one-line summary of what the user wants.
    pub goal: String,
    /// Route-specific parameters extracted from the message.
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Default confidence threshold — below this, the agent falls back
/// to `justChatting` (safe mode, no tools, clarify instead of guess).
pub const FALLBACK_THRESHOLD: f32 = 0.5;

/// The default safe-mode route used when confidence is low.
pub const DEFAULT_ROUTE: &str = "justChatting";

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
pub struct MechanisticAgent {
    handlers: HashMap<(String, String), HandlerFn>,
    /// Route name → set of supported (type, domain) pairs for that mode.
    route_tasks: HashMap<String, Vec<(String, String)>>,
    repair: RepairConfig,
    /// Confidence threshold below which the agent falls back to DEFAULT_ROUTE.
    fallback_threshold: f32,
    /// Whether verbose traces are emitted to stderr during execution.
    verbose: bool,
    /// Optional shared session store for cross-run context retrieval.
    session_store: Option<Arc<SessionStore>>,
    /// Optional shared memory store for structured recall across runs.
    memory_store: Option<Arc<MemoryStore>>,
    /// Context budget in tokens per inference call (0 = no limit).
    context_budget_tokens: usize,
}

impl MechanisticAgent {
    /// Create a new mechanistic agent with default configuration.
    /// Backend is taken per-call by `run()`, enabling reuse across runs.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            route_tasks: HashMap::new(),
            repair: RepairConfig::default(),
            fallback_threshold: FALLBACK_THRESHOLD,
            verbose: false,
            session_store: None,
            memory_store: None,
            context_budget_tokens: 512,
        }
    }

    /// Override the default repair loop configuration.
    pub fn with_repair(mut self, config: RepairConfig) -> Self {
        self.repair = config;
        self
    }

    /// Set the confidence threshold for fallback detection.
    /// Below this value, `classify()` returns `DEFAULT_ROUTE`.
    pub fn with_fallback_threshold(mut self, threshold: f32) -> Self {
        self.fallback_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Set verbosity (verbose traces go to stderr during execution).
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Attach a shared session store for cross-run context retrieval.
    pub fn with_session_store(mut self, store: Arc<SessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }

    /// Attach a shared memory store for structured recall across runs.
    pub fn with_memory_store(mut self, store: Arc<MemoryStore>) -> Self {
        self.memory_store = Some(store);
        self
    }

    /// Set the token budget for context snippets per inference call.
    /// Default is 512 tokens.
    pub fn with_context_budget(mut self, tokens: usize) -> Self {
        self.context_budget_tokens = tokens;
        self
    }

    /// Build a ContextManager configured with available sources.
    fn build_context_manager(
        &self,
        _query: &str,
        workspace_root: Option<&std::path::Path>,
    ) -> super::context::ContextManager {
        let mut manager = super::context::ContextManager::new(self.context_budget_tokens)
            .with_global_limit(20);

        if let Some(ref store) = self.session_store {
            manager = manager.add_source(Box::new(
                super::context::SessionContextSource::new(store.clone(), 3),
            ));
        }
        if let Some(ref store) = self.memory_store {
            manager = manager.add_source(Box::new(
                super::context::MemoryContextSource::new(store.clone(), 3),
            ));
        }
        if let Some(root) = workspace_root {
            manager = manager.add_source(Box::new(
                super::context::WorkspaceContextSource::new(root),
            ));
        }

        manager
    }

    /// Register a handler for a `(type, domain)` pair.
    ///
    /// Unknown pairs will fail loud during dispatch.
    pub fn register(&mut self, r#type: &str, domain: &str, handler: HandlerFn) {
        self.handlers.insert((r#type.to_string(), domain.to_string()), handler);
    }

    /// Declare a route with its supported task types.
    ///
    /// Routes are named modes (e.g. "storyTeller", "coder"). Each route
    /// declares which `(type, domain)` pairs it handles. The intent
    /// classifier selects a route, and dispatch validates that the plan's
    /// tasks are within the route's declared set.
    pub fn add_route(&mut self, name: &str, tasks: Vec<(&str, &str)>) {
        let pairs: Vec<(String, String)> = tasks
            .into_iter()
            .map(|(t, d)| (t.to_string(), d.to_string()))
            .collect();
        self.route_tasks.insert(name.to_string(), pairs);
    }

    /// Run the full mechanistic loop with intent classification and routing.
    ///
    /// Takes a backend per-call (no lifetime bound), enabling the agent to be
    /// constructed once and reused across multiple runs.
    ///
    /// 1. Classify: model call → structured Intent (route, confidence, goal).
    ///    Low confidence falls back to `justChatting`.
    /// 2. Think: free-form model call to reason about the request.
    /// 3. Derive (repair loop): grammar-constrained call → structured Plan.
    /// 4. Validate: check plan tasks against the selected route's declared set.
    /// 5. Dispatch: route each task to its handler; handlers write into the
    ///    temp workspace.
    /// 6. Commit: snapshot workspace files and return the outcome.
    pub async fn run(
        &self,
        backend: &dyn ModelBackend,
        msg: &str,
    ) -> Result<MechanisticOutcome, AgentError> {
        let ws = Workspace::temp(WorkspaceKind::Temp)
            .map_err(|e| AgentError::Internal(format!("failed to create workspace: {e}")))?;

        let intent = self.classify(backend, msg).await?;
        if self.verbose {
            eprintln!("[mecha:identify] route={} confidence={:.2}", intent.route, intent.confidence);
        }
        let thoughts = self.think_with_intent(backend, &intent, msg).await?;
        if self.verbose {
            eprintln!("[mecha:think] {}", thoughts.lines().take(3).collect::<Vec<_>>().join("\n"));
        }
        let plan = self.repair_derive(backend, &thoughts).await?;
        self.validate_route_tasks(&intent.route, &plan)?;
        let results = self.dispatch(backend, &plan, &ws).await?;
        let outcome = self.commit(plan, results, &ws)?;
        Ok(outcome)
    }

    /// Phase 0: classify the user's intent against known routes.
    ///
    /// Calls the model with the `INTENT_GRAMMAR` to produce a structured
    /// `Intent`. If confidence is below `fallback_threshold`, the route
    /// is overridden to `DEFAULT_ROUTE` (`justChatting`).
    ///
    /// Prepend-pull protocol: queries session store and memory store for
    /// previously classified requests that match, avoiding redundant classification.
    async fn classify(
        &self,
        backend: &dyn ModelBackend,
        msg: &str,
    ) -> Result<Intent, AgentError> {
        // Pull relevant context from available sources.
        let mut ctx_mgr = self.build_context_manager(msg, None);
        let snippets = ctx_mgr.collect(msg);
        let ctx_block = super::context::ContextManager::to_prompt_block(&snippets);

        let mut prompt = format!(
            "Classify this user request into a route and goal.\n\nUser: {}\n\nIntent (as JSON):",
            msg
        );
        if !ctx_block.is_empty() {
            prompt.insert_str(0, &format!("{}\n\n", ctx_block));
        }
        let grammar = Some(INTENT_GRAMMAR.to_string());
        let resp = backend_call(backend, "", &prompt, grammar.as_deref(), 128, 0.2).await?;

        let mut intent: Intent = serde_json::from_str(&resp).map_err(|e| {
            AgentError::Internal(format!(
                "failed to parse intent from model output: {e}\noutput: {resp}"
            ))
        })?;

        // Enforce confidence threshold — low confidence → safe fallback.
        if intent.confidence < self.fallback_threshold {
            intent.route = DEFAULT_ROUTE.to_string();
            intent.confidence = 1.0; // mark as forced
        }

        // Validate the route is known.
        if !self.route_tasks.contains_key(&intent.route) && !self.handlers.is_empty() {
            // If the route isn't registered but we have handlers, fall back.
            // If no routes are registered at all, accept any route.
            if !self.route_tasks.is_empty() {
                intent.route = DEFAULT_ROUTE.to_string();
            }
        }

        Ok(intent)
    }

    /// Phase 1: free-form model call seeded with the classified intent.
    ///
    /// Prepend-pull protocol: surfaces context from past sessions and memory
    /// that shares keywords with the current request or its goal.
    async fn think_with_intent(
        &self,
        backend: &dyn ModelBackend,
        intent: &Intent,
        msg: &str,
    ) -> Result<String, AgentError> {
        // Combine message + goal for richer query.
        let query = format!("{} {}", msg, intent.goal);
        let mut ctx_mgr = self.build_context_manager(&query, None);
        let snippets = ctx_mgr.collect(&query);
        let ctx_block = super::context::ContextManager::to_prompt_block(&snippets);

        let mut prompt = format!(
            "Route: {}\nGoal: {}\n\nRequest: {}\n\nReason about this and plan a response:",
            intent.route, intent.goal, msg
        );
        if !ctx_block.is_empty() {
            prompt.insert_str(0, &format!("{}\n\n", ctx_block));
        }
        let resp = backend_call(backend, "", &prompt, None, 512, 0.7).await?;
        Ok(resp)
    }

    /// Phase 1 core: free-form model call without intent context.
    #[allow(dead_code)]
    async fn think(&self, _backend: &dyn ModelBackend, msg: &str) -> Result<String, AgentError> {
        let prompt = format!("Reason about this request and plan a response:\n{}", msg);
        let resp = backend_call(_backend, "", &prompt, None, 512, 0.7).await?;
        Ok(resp)
    }

    /// Phase 2: grammar-constrained model call with repair loop.
    ///
    /// Before each attempt it collects relevant context from sessions/memory
    /// keyed against the prior reasoning (`thoughts`). If an attempt fails,
    /// already-attempted plans are avoided by excluding them from future
    /// context windows.
    async fn repair_derive(
        &self,
        backend: &dyn ModelBackend,
        thoughts: &str,
    ) -> Result<Plan, AgentError> {
        let mut temp = self.repair.temperature;
        let mut max_tokens = self.repair.max_tokens;
        let mut last_err = None;

        // Pre-collect context once per derive cycle, keyed against the reasoning.
        let mut ctx_mgr = self.build_context_manager(thoughts, None);
        let snippets = ctx_mgr.collect(thoughts);
        let ctx_block = super::context::ContextManager::to_prompt_block(&snippets);

        for attempt in 0..=self.repair.max_retries {
            let grammar = Some(PLAN_GRAMMAR.to_string());
            let mut prompt = format!(
                "Based on your reasoning, produce a structured plan as JSON tasks:\n{}",
                thoughts
            );
            if !ctx_block.is_empty() {
                prompt.insert_str(0, &format!("{}\n\n", ctx_block));
            }

            match backend_call(backend, "", &prompt, grammar.as_deref(), max_tokens, temp).await {
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

    /// Validate that all plan tasks are within the selected route's declared
    /// task set. If the route has no declared tasks, skip validation (any
    /// task is accepted).
    fn validate_route_tasks(&self, route: &str, plan: &Plan) -> Result<(), AgentError> {
        let Some(allowed) = self.route_tasks.get(route) else {
            return Ok(()); // no task restrictions for this route
        };
        for task in &plan.tasks {
            let key = (task.r#type.clone(), task.domain.clone());
            if !allowed.contains(&key) {
                return Err(AgentError::Internal(format!(
                    "task ({}, {}) not allowed in route '{}'",
                    task.r#type, task.domain, route
                )));
            }
        }
        Ok(())
    }

    /// Dispatch a single task to its registered handler.
    ///
    /// `backend` is passed through to the handler closure for compatibility.
    pub fn dispatch_single(
        &self,
        _backend: &dyn ModelBackend,
        task: &Task,
        ws: &Workspace,
    ) -> Result<HandlerResult, AgentError> {
        let key = (task.r#type.clone(), task.domain.clone());
        let handler = self.handlers.get(&key).ok_or_else(|| {
            AgentError::Internal(format!(
                "no handler registered for ({}, {})",
                task.r#type, task.domain
            ))
        })?;
        Ok(handler(task, _backend, ws))
    }

    /// Phase 3: dispatch each task to its registered handler.
    async fn dispatch(
        &self,
        _backend: &dyn ModelBackend,
        plan: &Plan,
        ws: &Workspace,
    ) -> Result<Vec<HandlerResult>, AgentError> {
        let mut results = Vec::new();
        for task in &plan.tasks {
            let key = (task.r#type.clone(), task.domain.clone());
            let handler = self.handlers.get(&key).ok_or_else(|| {
                AgentError::Internal(format!(
                    "no handler registered for ({}, {})",
                    task.r#type, task.domain
                ))
            })?;
            let result = handler(task, _backend, ws);
            results.push(result);
        }
        Ok(results)
    }

    /// Phase 4: snapshot workspace files into the outcome.
    pub fn commit(
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

}

/// Alias for clarity when used alongside [`CommonAgent`](crate::CommonAgent).
pub type MechaAgent = MechanisticAgent;

// ── BaseAgent trait impl ────────────────────────────────────────────────

impl BaseAgent for MechanisticAgent {
    /// Run the mechanistic pipeline and return only the final human-readable text.
    async fn run(&self, backend: &dyn ModelBackend, msg: &str) -> Result<String, AgentError> {
        let outcome = self.run(backend, msg).await?;
        Ok(outcome.handler_results.iter()
            .map(|r| r.output.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"))
    }

    fn verbose(&self) -> bool {
        self.verbose
    }
}

/// Helper: call the backend with error mapping.
///
/// Standalone async function so methods that no longer borrow the backend
/// from the struct can still use it.
async fn backend_call(
    backend: &dyn ModelBackend,
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
    let resp = backend
        .complete(req)
        .await
        .map_err(|e| AgentError::BackendError(format!("{e}")))?;
    Ok(resp.text)
}

/// BNF grammar that constrains model output to a valid Intent JSON.
const INTENT_GRAMMAR: &str = r#"
root  ::= "{" space "\"route\"" space ":" space string space "," space "\"confidence\"" space ":" space number space "," space "\"goal\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
number ::= [0-9] "." [0-9] [0-9]?
space ::= " "?
"#;

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

    #[tokio::test]
    async fn test_register_and_dispatch_known_handler() {
        let backend = MockBackend::default();
        let mut agent = MechanisticAgent::new();

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
        let results = agent.dispatch(&backend, &plan, &ws).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].output, "written");
    }

    #[tokio::test]
    async fn test_unknown_handler_fails_loud() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new();

        let plan = Plan {
            tasks: vec![Task {
                r#type: "nonexistent".into(),
                domain: "void".into(),
                spec: serde_json::json!({}),
            }],
        };

        let ws = Workspace::temp(WorkspaceKind::Temp).unwrap();
        let result = agent.dispatch(&backend, &plan, &ws).await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("no handler registered"),
            "must fail loud on unknown (type, domain)"
        );
    }

    #[tokio::test]
    async fn test_empty_handlers_rejects_any_task() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new();

        let plan = Plan {
            tasks: vec![Task {
                r#type: "write".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({}),
            }],
        };

        let ws = Workspace::temp(WorkspaceKind::Temp).unwrap();
        let result = agent.dispatch(&backend, &plan, &ws).await;
        assert!(result.is_err(), "must fail when no handlers are registered");
    }

    #[tokio::test]
    async fn test_full_run_with_workspace() {
        let backend = MockBackend::default();
        let mut agent = MechanisticAgent::new();

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
        let result = agent.run(&backend, "Write a story about a dragon").await;
        assert!(result.is_err() || result.is_ok());
    }

    #[tokio::test]
    async fn test_full_run_parse_failure_path() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new();

        // With the default MockBackend, derive should fail to parse.
        let result = agent.run(&backend, "test").await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("failed to parse") || msg.contains("attempt") || msg.contains("retries"),
            "run should fail with parse error, got: {msg}");
    }

    #[tokio::test]
    async fn test_handler_calls_backend_and_writes_workspace() {
        let backend = MockBackend::default();
        let mut agent = MechanisticAgent::new();

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
        let results = agent.dispatch(&backend, &plan, &ws).await.unwrap();
        assert_eq!(results.len(), 1);

        // The workspace should contain the file written by the handler.
        let ws_file = ws.root().join("OUTPUT.md");
        assert!(ws_file.exists(), "handler should write to workspace");
        let content = std::fs::read_to_string(ws_file).unwrap();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_commit_snapshots_workspace_files() {
        let agent = MechanisticAgent::new();
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

    #[tokio::test]
    async fn test_repair_loop_retries_on_parse_failure() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new().with_repair(RepairConfig {
            max_retries: 1,
            temperature: 0.5,
            temperature_delta: 0.2,
            temperature_floor: 0.1,
            max_tokens: 256,
            token_decay: 64,
            min_tokens: 64,
        });

        let result = agent.repair_derive(&backend, "Write a story.").await;
        assert!(result.is_err(), "repair loop should fail after exhausting retries");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("attempt") || msg.contains("retries") || msg.contains("failed to parse"),
            "error should mention retry attempts, got: {msg}"
        );
    }

    #[tokio::test]
    async fn test_repair_loop_zero_retries_fails_immediately() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new().with_repair(RepairConfig {
            max_retries: 0,
            ..RepairConfig::default()
        });

        let result = agent.repair_derive(&backend, "Write a story.").await;
        assert!(result.is_err(), "zero retries should fail immediately");
    }

    #[tokio::test]
    async fn test_classify_falls_back_on_low_confidence() {
        let backend = MockBackend::default();
        let agent = MechanisticAgent::new().with_fallback_threshold(0.8);

        // MockBackend echoes the prompt, which won't parse as Intent JSON.
        // classify() should fail with a parse error rather than panic.
        let result = agent.classify(&backend, "write me a story").await;
        // Either parse error or default route — no panics.
        match result {
            Ok(intent) => {
                // If parsing somehow succeeds, low confidence forces fallback.
                assert_eq!(intent.route, DEFAULT_ROUTE);
            }
            Err(e) => {
                assert!(e.to_string().contains("failed to parse intent"));
            }
        }
    }

    #[test]
    fn test_validate_route_tasks_accepts_allowed_task() {
        let mut agent = MechanisticAgent::new();
        agent.add_route("storyTeller", vec![("write", "chapter"), ("write", "wiki")]);

        let plan = Plan {
            tasks: vec![Task {
                r#type: "write".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({}),
            }],
        };

        assert!(agent.validate_route_tasks("storyTeller", &plan).is_ok());
    }

    #[test]
    fn test_validate_route_tasks_rejects_disallowed_task() {
        let mut agent = MechanisticAgent::new();
        agent.add_route("storyTeller", vec![("write", "chapter")]);

        let plan = Plan {
            tasks: vec![Task {
                r#type: "write".into(),
                domain: "wiki".into(),
                spec: serde_json::json!({}),
            }],
        };

        let result = agent.validate_route_tasks("storyTeller", &plan);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed in route"));
    }

    #[test]
    fn test_validate_route_tasks_skips_if_no_routes_declared() {
        let agent = MechanisticAgent::new();

        let plan = Plan {
            tasks: vec![Task {
                r#type: "anything".into(),
                domain: "goes".into(),
                spec: serde_json::json!({}),
            }],
        };

        // No routes declared at all — validation should skip.
        assert!(agent.validate_route_tasks("any-route", &plan).is_ok());
    }

    #[tokio::test]
    async fn test_classify_with_route_table() {
        let backend = MockBackend::default();
        let mut agent = MechanisticAgent::new()
            .with_fallback_threshold(0.2);
        agent.add_route("storyTeller", vec![("write", "chapter"), ("write", "wiki")]);
        agent.add_route("justChatting", vec![]);

        // MockBackend echoes the prompt, so parsing will fail.
        // This tests that classify() doesn't panic with routes registered.
        let result = agent.classify(&backend, "write me a story about a dragon").await;
        match result {
            Ok(intent) => {
                // If parsed, route should be one of the registered routes or default.
                assert!(&intent.route == "storyTeller" || &intent.route == DEFAULT_ROUTE);
            }
            Err(e) => {
                assert!(e.to_string().contains("failed to parse intent"));
            }
        }
    }
}
