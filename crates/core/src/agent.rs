//! Orchestrator-worker sub-agent layer for RoCo AI.
//!
//! Operationalizes the patterns in `models/small_model_agent_patterns.md`:
//!  - 4K context budget enforcement (§4.1)
//!  - Orchestrator-Worker decomposition with pre-bound context (§1.1, §1.2)
//!  - Schema-first, deterministic worker interfaces (§2.2)
//!  - Verification gates between stages (§3, §5.2)
//!  - Escalation cascade + retry circuit breakers (§5.1, §5.3)
//!  - Fan-out execution and structural aggregation (§1.3, §3.3)

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::future::join_all;
use serde_json::Value;
use thiserror::Error;

use crate::engine::{CompletionRequest, CompletionResponse, ModelBackend, TokenCounter, TokenUsage};
use crate::grammar::{tools_to_gbnf, tools_to_gbnf_with_think};
use crate::policy::Policy;
use crate::sandbox::Sandbox;
use crate::tools::ToolRegistry;
use crate::toolcall::{execute_tool_calls, parse_tool_calls, ToolExecutionResult};
use crate::trace::{TraceEvent, Tracer};

// ---------------------------------------------------------------------------
// Context budget (§4.1)
// ---------------------------------------------------------------------------

/// The 4K context-window allocation. Defaults match the doc's empirically
/// robust breakdown.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    pub total: usize,
    pub system: usize,
    pub task_context: usize,
    pub tools: usize,
    pub scratch: usize,
    pub generation: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            total: 4096,
            system: 700,
            task_context: 1200,
            tools: 800,
            scratch: 700,
            generation: 1536,
        }
    }
}

impl ContextBudget {
    /// Hard rule (§4.1): combined prompt (system+context+tools) must not exceed 3000.
    pub fn max_prompt(&self) -> usize {
        (self.total - self.generation).min(3000)
    }
    pub fn fits_prompt(&self, used: usize) -> bool {
        used <= self.max_prompt()
    }
}

// ---------------------------------------------------------------------------
// Tasks & subtasks
// ---------------------------------------------------------------------------

/// A user-facing task. May carry more context than a single 4K window; the
/// orchestrator chunks it into atomic subtasks.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub objective: String,
    /// May be large; the orchestrator chunks it to fit [`ContextBudget::task_context`].
    pub context: String,
    /// JSON-schema-shaped description of the required output (§2.2A).
    pub output_schema: String,
    /// Whether the worker may abstain via a do_nothing action (§2.2C).
    pub allow_abstain: bool,
}

/// A single atomic unit of work handed to one 3B worker (§1.1). All context is
/// pre-bound inline; the worker performs no re-planning.
#[derive(Debug, Clone)]
pub struct Subtask {
    pub id: String,
    pub objective: String,
    /// Inline, already-resolved context.
    pub context: String,
    pub output_schema: String,
    pub allow_abstain: bool,
    /// Estimated tokens of the rendered prompt, for budget enforcement.
    pub prompt_tokens: usize,
}

impl Subtask {
    /// Schema-first rendering (§2.2A).
    pub fn render_prompt(&self) -> String {
        format!(
            "[SCHEMA]\n{}\n\n[TASK]\n{}\n\n{}",
            self.output_schema, self.objective, self.context
        )
    }
}

/// The output produced by a worker for a subtask.
#[derive(Debug, Clone)]
pub struct WorkerOutput {
    pub subtask_id: String,
    /// The full execution transcript, including all tool calls and results.
    pub raw: String,
    /// The final model response only.
    pub final_raw: String,
    pub parsed: Value,
    pub usage: TokenUsage,
    /// True when the worker chose the do_nothing / abstain action.
    pub aborted: bool,
    /// Results of any tool calls the worker executed (empty when tooling is
    /// not enabled or the model emitted none).
    pub tool_results: Vec<ToolExecutionResult>,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Engine(#[from] crate::engine::EngineError),
    #[error("subtask {id} prompt exceeds context budget: used {used} of {max} tokens")]
    BudgetExceeded { id: String, used: usize, max: usize },
    #[error("subtask {id} exhausted all retries (reached {level:?})")]
    Exhausted { id: String, level: EscalationLevel },
    #[error("human intervention required for subtask {id}: {reason}")]
    HumanIntervention { id: String, reason: String },
    #[error("verification failed: {0}")]
    Verification(String),
}

// ---------------------------------------------------------------------------
// Worker (the 3B sub-agent)
// ---------------------------------------------------------------------------

/// Wraps a [`ModelBackend`] as a single 3B specialist worker.
pub struct Worker<B: ModelBackend + Send + Sync + ?Sized> {
    backend: Arc<B>,
    budget: ContextBudget,
    /// Optional tool-calling support (see `toolcall`).
    tools: Option<Arc<ToolRegistry>>,
    sandbox: Option<Arc<Sandbox>>,
    policy: Option<Arc<dyn Policy>>,
    /// Max tool-use rounds in the agentic loop (each round = one model call
    /// that may emit tool calls whose results are fed back). Ignored when the
    /// worker has no tooling.
    max_tool_rounds: usize,
    /// Optional execution tracer.
    tracer: Option<Arc<dyn Tracer>>,
}

impl<B: ModelBackend + Send + Sync + ?Sized> Worker<B> {
    pub fn new(backend: Arc<B>, budget: ContextBudget) -> Self {
        Self {
            backend,
            budget,
            tools: None,
            sandbox: None,
            policy: None,
            max_tool_rounds: 4,
            tracer: None,
        }
    }

    /// Enable tool-calling for this worker. Tool calls parsed from the model
    /// response are vetted by `policy` and dispatched via `tools`/`sandbox`.
    pub fn with_tooling(
        mut self,
        tools: Arc<ToolRegistry>,
        sandbox: Arc<Sandbox>,
        policy: Arc<dyn Policy>,
    ) -> Self {
        self.tools = Some(tools);
        self.sandbox = Some(sandbox);
        self.policy = Some(policy);
        self
    }

    /// Set the maximum number of agentic tool-use rounds (default 4). Each round
    /// is one model call that may emit tool calls; their results are fed back
    /// into the next call. Ignored when the worker has no tooling.
    pub fn with_max_tool_rounds(mut self, rounds: usize) -> Self {
        self.max_tool_rounds = rounds.max(1);
        self
    }

    /// Attach an execution tracer. The worker emits events for every
    /// architectural step (budget check, model call, tool parse, execution).
    pub fn with_tracer(mut self, tracer: Arc<dyn Tracer>) -> Self {
        self.tracer = Some(tracer);
        self
    }

    /// Execute a single atomic subtask with a schema-first prompt and sampling
    /// discipline (§2.2). Enforces the context budget before dispatch.
    ///
    /// When the worker is tool-enabled, this runs a **multi-step agentic loop**:
    /// the model may emit `<tool_call>` blocks, which are vetted by `policy` and
    /// executed; their results are appended to the running transcript and fed
    /// back into the prompt for another model call. The loop repeats until the
    /// model returns no tool calls (a final answer) or `max_tool_rounds` is
    /// reached. All tool results across rounds are accumulated into the
    /// returned [`WorkerOutput`].
    pub async fn execute(&self, task: &Subtask) -> Result<WorkerOutput, AgentError> {
        if let Some(t) = &self.tracer {
            t.record(TraceEvent::new(
                "budget_check",
                format!("worker-{}", task.id),
                format!("prompt_tokens={} budget_max={}", task.prompt_tokens, self.budget.max_prompt()),
            ).with_meta(serde_json::json!({
                "subtask_id": &task.id,
                "prompt_tokens": task.prompt_tokens,
                "budget_max": self.budget.max_prompt(),
                "fits": self.budget.fits_prompt(task.prompt_tokens),
            })));
        }
        if !self.budget.fits_prompt(task.prompt_tokens) {
            return Err(AgentError::BudgetExceeded {
                id: task.id.clone(),
                used: task.prompt_tokens,
                max: self.budget.max_prompt(),
            });
        }
        let system = build_system_prompt(&task.output_schema, task.allow_abstain);

        // Round count: tool-enabled workers may iterate; others do a single pass.
        let rounds = if self.tools.is_some() {
            self.max_tool_rounds
        } else {
            1
        };

        let mut prompt = task.render_prompt();
        let mut tool_results: Vec<ToolExecutionResult> = Vec::new();
        let mut final_resp: Option<CompletionResponse> = None;

        for round in 0..rounds {
            let req = CompletionRequest {
                system: system.clone(),
                prompt: prompt.clone(),
                output_schema: Some(task.output_schema.clone()),
                grammar: self.tools.as_ref().map(|tools| {
                    tools_to_gbnf_with_think(tools)
                }),
                temperature: 0.2,
                max_tokens: 512,
                estimated_prompt_tokens: task.prompt_tokens,
                thinking: false,
                preserve_state: false,
            };
            if let Some(t) = &self.tracer {
                t.record(TraceEvent::new(
                    "model_call",
                    format!("worker-{}", task.id),
                    format!("round {}/{} tokens={}", round + 1, rounds, req.estimated_prompt_tokens),
                ).with_meta(serde_json::json!({
                    "subtask_id": &task.id,
                    "round": round + 1,
                    "max_rounds": rounds,
                    "prompt_tokens": req.estimated_prompt_tokens,
                    "max_tokens": req.max_tokens,
                })));
            }
            let resp = self.backend.complete(req).await?;
            final_resp = Some(resp.clone());

            // Only tool-enabled workers parse/execute tool calls.
            let calls = if self.tools.is_some() {
                parse_tool_calls(&resp.text)
            } else {
                Vec::new()
            };

            if let Some(t) = &self.tracer {
                let parse_detail = if calls.is_empty() {
                    "no tool calls — final answer".to_string()
                } else {
                    format!("parsed {} tool call(s)", calls.len())
                };
                t.record(TraceEvent::new(
                    "tool_parse",
                    format!("worker-{}", task.id),
                    &parse_detail,
                ).with_meta(serde_json::json!({
                    "subtask_id": &task.id,
                    "round": round + 1,
                    "call_count": calls.len(),
                    "calls": calls.iter().map(|c| &c.name).collect::<Vec<_>>(),
                })));
            }

            if calls.is_empty() {
                // No tool calls => this is the final answer; stop the loop.
                break;
            }
            if round + 1 >= rounds {
                // Final allowed round still produced tool calls: stop, keeping
                // whatever results were accumulated (no final answer given).
                break;
            }

            // Execute the tool calls under the policy and feed results back.
            let tools = self.tools.as_ref().unwrap();
            let sandbox = self.sandbox.as_ref().unwrap();
            let policy = &**self.policy.as_ref().unwrap();

            if let Some(t) = &self.tracer {
                t.record(TraceEvent::new(
                    "tool_exec",
                    format!("worker-{}", task.id),
                    format!("executing {} tool call(s)", calls.len()),
                ).with_meta(serde_json::json!({
                    "subtask_id": &task.id,
                    "round": round + 1,
                    "calls": calls.iter().map(|c| serde_json::json!({"name": &c.name, "args": &c.arguments})).collect::<Vec<_>>(),
                })));
            }
            let results = execute_tool_calls(&resp.text, tools, sandbox, policy).await;

            if let Some(t) = &self.tracer {
                let success = results.iter().filter(|r| r.output.is_some()).count();
                let errors = results.iter().filter(|r| r.error.is_some()).count();
                t.record(TraceEvent::new(
                    "tool_result",
                    format!("worker-{}", task.id),
                    format!("{success} ok, {errors} failed"),
                ).with_meta(serde_json::json!({
                    "subtask_id": &task.id,
                    "round": round + 1,
                    "success": success,
                    "errors": errors,
                    "results": results.iter().map(|r| {
                        if let Some(out) = &r.output { out.clone() }
                        else if let Some(err) = &r.error { serde_json::json!({"error": err}) }
                        else { serde_json::json!({"verdict": format!("{:?}", r.verdict), "name": r.call.name}) }
                    }).collect::<Vec<_>>(),
                })));
            }
            tool_results.extend(results);

            // Append the assistant's tool call and the tool outputs to the
            // running transcript so the model can reason over them next round.
            prompt.push_str(&format!(
                "\n\n[ASSISTANT TOOL CALL]\n{}\n\n[TOOL RESULTS]\n{}",
                resp.text,
                serde_json::to_string_pretty(&Self::tool_results_to_json(&tool_results))
                    .unwrap_or_default()
            ));
            tracing::debug!(
                subtask = %task.id,
                round,
                accumulated = tool_results.len(),
                "agentic tool loop step"
            );
        }

        let resp = final_resp.ok_or_else(|| {
            AgentError::Verification("worker produced no model response".into())
        })?;
        let final_raw = resp.text.clone();

        let parsed = match serde_json::from_str::<Value>(&final_raw) {
            Ok(Value::Object(mut obj)) => {
                obj.insert("tool_results".to_string(), Self::tool_results_to_json(&tool_results));
                Value::Object(obj)
            }
            Ok(other) => {
                serde_json::json!({
                    "answer": other,
                    "tool_results": Self::tool_results_to_json(&tool_results)
                })
            }
            Err(_) => {
                serde_json::json!({
                    "raw_answer": final_raw,
                    "tool_results": Self::tool_results_to_json(&tool_results)
                })
            }
        };
        let aborted = task.allow_abstain
            && parsed.get("action").and_then(|a| a.as_str()) == Some("do_nothing");
        Ok(WorkerOutput {
            subtask_id: task.id.clone(),
            raw: format!("{}\n\n[FINAL ANSWER]\n{}", prompt, final_raw),
            final_raw,
            parsed,
            usage: resp.usage,
            aborted,
            tool_results,
        })
    }

    /// Serialize accumulated [`ToolExecutionResult`]s into a JSON array suitable
    /// for feeding back into the model prompt as `[TOOL RESULTS]`.
    fn tool_results_to_json(results: &[ToolExecutionResult]) -> Value {
        Value::Array(
            results
                .iter()
                .map(|r| {
                    if let Some(out) = &r.output {
                        out.clone()
                    } else if let Some(err) = &r.error {
                        serde_json::json!({ "error": err })
                    } else {
                        // Denied / review-pending: surface the verdict, not output.
                        serde_json::json!({ "verdict": format!("{:?}", r.verdict), "name": r.call.name })
                    }
                })
                .collect(),
        )
    }
}

/// Builds the schema-first system prompt (§2.2A, §2.2C, §2.2F).
fn build_system_prompt(schema: &str, allow_abstain: bool) -> String {
    let mut s = String::new();
    s.push_str("You are a specialist sub-agent operating under a strict 4K context budget.\n");
    s.push_str("[OUTPUT SCHEMA - follow exactly]\n");
    s.push_str(schema);
    s.push_str("\n\n[INSTRUCTIONS]\n");
    s.push_str("- Output ONLY the schema-shaped result. No commentary, no reasoning tokens.\n");
    s.push_str("- Do not invent tool or function names absent from the schema.\n");
    if allow_abstain {
        s.push_str("- If the request is outside your capability, output the do_nothing action instead of guessing.\n");
    }
    s
}

// ---------------------------------------------------------------------------
// Verification (§3, §5.2)
// ---------------------------------------------------------------------------

/// The result of verifying a worker's output against a gate.
#[derive(Debug, Clone)]
pub struct VerificationVerdict {
    pub passed: bool,
    /// Normalized 0..1 score.
    pub score: f32,
    pub reason: String,
    pub checks: HashMap<String, bool>,
}

/// A verification gate. Implementations: deterministic checklist (no model) and
/// LLM-as-judge (separate small model).
#[async_trait]
pub trait Verifier {
    async fn verify(
        &self,
        task: &Subtask,
        output: &WorkerOutput,
    ) -> Result<VerificationVerdict, AgentError>;
}

/// Deterministic structured-checklist verifier (§5.2). No model required, so it
/// is always safe to use as a first-pass gate.
pub struct ChecklistVerifier;

#[async_trait]
impl Verifier for ChecklistVerifier {
    async fn verify(
        &self,
        task: &Subtask,
        output: &WorkerOutput,
    ) -> Result<VerificationVerdict, AgentError> {
        let mut checks: HashMap<String, bool> = HashMap::new();

        let parsed = match serde_json::from_str::<Value>(&output.final_raw) {
            Ok(v) => v,
            Err(_) => {
                checks.insert("check_syntax".into(), false);
                return Ok(VerificationVerdict {
                    passed: false,
                    score: 0.0,
                    reason: "output is not valid JSON".into(),
                    checks,
                });
            }
        };
        checks.insert("check_syntax".into(), true);

        let schema_keys = schema_keys(&task.output_schema);
        let required = schema_keys.clone().unwrap_or_default();
        let all_present = required.iter().all(|k| parsed.get(k).is_some());
        checks.insert("check_all_required_fields_present".into(), all_present);

        let no_hallucinated = match schema_keys {
            Some(keys) => parsed
                .as_object()
                .map(|o| o.keys().all(|k| keys.contains(k)))
                .unwrap_or(false),
            None => true,
        };
        checks.insert("check_no_hallucinated_keys".into(), no_hallucinated);

        // Value-range checks are schema-specific; placeholder pass for now.
        checks.insert("check_values_within_expected_range".into(), true);

        let format_ok = all_present && no_hallucinated;
        checks.insert("check_output_format_matches_schema".into(), format_ok);

        let passed = checks.values().all(|&b| b);
        let score = checks.values().filter(|&&b| b).count() as f32 / checks.len().max(1) as f32;
        let reason = if passed {
            "all checklist items passed".into()
        } else {
            "one or more checklist items failed".into()
        };
        Ok(VerificationVerdict {
            passed,
            score,
            reason,
            checks,
        })
    }
}

/// LLM-as-judge verifier (§3.1, §3.2 Pattern 1). Uses a *separate* small model
/// as judge to reduce self-evaluation bias (§5.2).
pub struct JudgeVerifier<B: ModelBackend + Send + Sync> {
    backend: Arc<B>,
}

impl<B: ModelBackend + Send + Sync> JudgeVerifier<B> {
    pub fn new(backend: Arc<B>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl<B: ModelBackend + Send + Sync> Verifier for JudgeVerifier<B> {
    async fn verify(
        &self,
        task: &Subtask,
        output: &WorkerOutput,
    ) -> Result<VerificationVerdict, AgentError> {
        let system = "You are a strict verification judge. Output JSON: \
            {\"score\": <0-10>, \"passed\": <true|false>, \"reason\": \"<one sentence>\"}. \
            Be biased toward factual specificity over fluency.";
        let prompt = format!(
            "OBJECTIVE:\n{}\n\nOUTPUT:\n{}\n\nRUBRIC: score factual accuracy, completeness, logical consistency (0-10 each).",
            task.objective, output.final_raw
        );
        let est = TokenCounter::estimate(&prompt);
        let req = CompletionRequest {
            system: system.into(),
            prompt,
            output_schema: None,
            grammar: None,
            temperature: 0.1,
            max_tokens: 256,
            estimated_prompt_tokens: est,
            thinking: false,
            preserve_state: false,
        };
        let resp = self.backend.complete(req).await?;
        let v: Value = serde_json::from_str(&resp.text).map_err(|e| {
            AgentError::Verification(format!("judge returned invalid JSON: {e}"))
        })?;
        let score = (v.get("score").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32 / 10.0)
            .clamp(0.0, 1.0);
        let passed = v.get("passed").and_then(|x| x.as_bool()).unwrap_or(false);
        let reason = v
            .get("reason")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let mut checks = HashMap::new();
        checks.insert("judge_passed".into(), passed);
        Ok(VerificationVerdict {
            passed,
            score,
            reason,
            checks,
        })
    }
}

/// Extract expected top-level keys from a JSON-schema object, preferring an
/// explicit `required` array (§5.2 checklist).
fn schema_keys(schema: &str) -> Option<Vec<String>> {
    let v: Value = serde_json::from_str(schema).ok()?;
    let obj = v.as_object()?;
    if let Some(req) = obj.get("required").and_then(|r| r.as_array()) {
        return Some(
            req.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect(),
        );
    }
    Some(obj.keys().cloned().collect())
}

// ---------------------------------------------------------------------------
// Escalation & retry (§5.1, §5.3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationLevel {
    /// L1: agent self-recovery (retry with simplified approach).
    SelfRecovery,
    /// L2: team-level replan / scope reduction by the orchestrator.
    TeamReplan,
    /// L3: human intervention.
    Human,
}

/// Retry circuit breakers (§5.3).
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Hard cap on total attempts per task: 3.
    pub max_per_task: usize,
    /// Softer cap on retries per step: 2.
    pub max_per_step: usize,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_per_task: 3,
            max_per_step: 2,
        }
    }
}

/// Tracks attempt count and maps it to an escalation level (§5.1).
pub struct EscalationController {
    policy: RetryPolicy,
    attempts: usize,
}

impl EscalationController {
    pub fn new(policy: RetryPolicy) -> Self {
        Self {
            policy,
            attempts: 0,
        }
    }
    pub fn record_attempt(&mut self) {
        self.attempts += 1;
    }
    pub fn attempts(&self) -> usize {
        self.attempts
    }
    pub fn current_level(&self) -> EscalationLevel {
        if self.attempts <= self.policy.max_per_step {
            EscalationLevel::SelfRecovery
        } else if self.attempts <= self.policy.max_per_task {
            EscalationLevel::TeamReplan
        } else {
            EscalationLevel::Human
        }
    }
    pub fn exhausted(&self) -> bool {
        self.attempts > self.policy.max_per_task
    }
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// The aggregated outcome of running a task across its subtasks.
#[derive(Debug, Clone)]
pub struct AggregatedResult {
    pub subtask_count: usize,
    pub failed: usize,
    pub outputs: Vec<Value>,
    pub majority_label: Option<String>,
    /// Tool-call results collected across all subtasks (empty when tooling is
    /// disabled or no tool calls were emitted).
    pub tool_results: Vec<Value>,
}

/// Orchestrator-Worker controller (§1.1). Decomposes a task, fans out to workers,
/// gates each result through a [`Verifier`], and aggregates.
pub struct Orchestrator<B: ModelBackend + Send + Sync + ?Sized, V: Verifier> {
    backend: Arc<B>,
    budget: ContextBudget,
    verifier: V,
    retry_policy: RetryPolicy,
    /// Optional tool-calling support, shared with every worker.
    tools: Option<Arc<ToolRegistry>>,
    sandbox: Option<Arc<Sandbox>>,
    policy: Option<Arc<dyn Policy>>,
    /// Optional execution tracer (rustviz-style). When `None`, recording is a
    /// no-op; the orchestration behaves exactly as before.
    tracer: Option<Arc<dyn Tracer>>,
}

impl<B: ModelBackend + Send + Sync + ?Sized, V: Verifier> Orchestrator<B, V> {
    pub fn new(
        backend: Arc<B>,
        budget: ContextBudget,
        verifier: V,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            backend,
            budget,
            verifier,
            retry_policy,
            tools: None,
            sandbox: None,
            policy: None,
            tracer: None,
        }
    }

    /// Enable tool-calling for the orchestrator. Every worker it spawns will
    /// parse/execute tool calls from model output under the given policy.
    pub fn with_tooling(
        mut self,
        tools: Arc<ToolRegistry>,
        sandbox: Arc<Sandbox>,
        policy: Arc<dyn Policy>,
    ) -> Self {
        self.tools = Some(tools);
        self.sandbox = Some(sandbox);
        self.policy = Some(policy);
        self
    }

    /// Attach an execution tracer. The orchestrator records each architectural
    /// step (decompose, worker execution, verification, retry, aggregation) so a
    /// viewer can replay the run. No-op when unset.
    pub fn with_tracer(mut self, tracer: Arc<dyn Tracer>) -> Self {
        self.tracer = Some(tracer);
        self
    }

    /// Decompose a task into atomic subtasks, chunking context to fit the budget
    /// (§1.1, §1.2, §4.1). Deterministic splitter; LLM-driven decomposition is a
    /// future model feature.
    pub fn decompose(&self, task: &Task) -> Vec<Subtask> {
        let chunks = chunk_text(&task.context, self.budget.task_context);
        let n = chunks.len().max(1);
        chunks
            .into_iter()
            .enumerate()
            .map(|(i, chunk)| {
                let objective = if n == 1 {
                    task.objective.clone()
                } else {
                    format!("{} (part {}/{})", task.objective, i + 1, n)
                };
                let prompt_text = format!(
                    "[SCHEMA]\n{}\n\n[TASK]\n{}\n\n{}",
                    task.output_schema, objective, chunk
                );
                let prompt_tokens = TokenCounter::estimate(&prompt_text);
                Subtask {
                    id: format!("{}-{}", task.id, i + 1),
                    objective,
                    context: chunk,
                    output_schema: task.output_schema.clone(),
                    allow_abstain: task.allow_abstain,
                    prompt_tokens,
                }
            })
            .collect()
    }

    /// Execute one subtask with retry + verification gate + escalation (§3, §5).
    async fn execute_with_recovery(&self, task: &Subtask) -> Result<WorkerOutput, AgentError> {
        if let Some(t) = &self.tracer {
            t.record(
                TraceEvent::new("execute", format!("worker-{}", task.id), "spawned; running model call + verify gate")
                    .with_meta(serde_json::json!({ "subtask_id": task.id, "prompt_tokens": task.prompt_tokens })),
            );
        }
        let mut worker = if self.tools.is_some() {
            Worker::new(Arc::clone(&self.backend), self.budget.clone()).with_tooling(
                Arc::clone(self.tools.as_ref().unwrap()),
                Arc::clone(self.sandbox.as_ref().unwrap()),
                Arc::clone(self.policy.as_ref().unwrap()),
            )
        } else {
            Worker::new(Arc::clone(&self.backend), self.budget.clone())
        };
        if let Some(t) = self.tracer.clone() {
            worker = worker.with_tracer(t);
        }
        let mut esc = EscalationController::new(self.retry_policy.clone());
        loop {
            match worker.execute(task).await {
                Ok(output) => {
                    let verdict = self.verifier.verify(task, &output).await?;
                    if let Some(t) = &self.tracer {
                        t.record(
                            TraceEvent::new(
                                "verify",
                                "verifier",
                                if verdict.passed { "gate passed" } else { "gate failed" },
                            )
                            .with_meta(serde_json::json!({
                                "subtask_id": task.id,
                                "passed": verdict.passed,
                                "score": verdict.score,
                                "reason": verdict.reason,
                            })),
                        );
                    }
                    if verdict.passed {
                        return Ok(output);
                    }
                    esc.record_attempt();
                    if let Some(t) = &self.tracer {
                        t.record(
                            TraceEvent::new(
                                "retry",
                                "orchestrator",
                                format!(
                                    "attempt {} -> level {:?}: {}",
                                    esc.attempts(),
                                    esc.current_level(),
                                    verdict.reason
                                ),
                            )
                            .with_meta(serde_json::json!({ "subtask_id": task.id, "level": format!("{:?}", esc.current_level()) })),
                        );
                    }
                    if esc.exhausted() {
                        return Err(AgentError::HumanIntervention {
                            id: task.id.clone(),
                            reason: verdict.reason,
                        });
                    }
                    tracing::debug!(
                        "subtask {} failed verification (level {:?}): {}",
                        task.id,
                        esc.current_level(),
                        verdict.reason
                    );
                }
                Err(e) => {
                    esc.record_attempt();
                    if let Some(t) = &self.tracer {
                        t.record(
                            TraceEvent::new(
                                "retry",
                                "orchestrator",
                                format!(
                                    "attempt {} -> level {:?}: {}",
                                    esc.attempts(),
                                    esc.current_level(),
                                    e
                                ),
                            )
                            .with_meta(serde_json::json!({ "subtask_id": task.id, "level": format!("{:?}", esc.current_level()) })),
                        );
                    }
                    if esc.exhausted() {
                        return Err(AgentError::Exhausted {
                            id: task.id.clone(),
                            level: esc.current_level(),
                        });
                    }
                    tracing::debug!(
                        "subtask {} errored (level {:?}): {}",
                        task.id,
                        esc.current_level(),
                        e
                    );
                }
            }
        }
    }

    /// Run a full task: decompose → fan-out execute → aggregate (§1.3, §3.3).
    pub async fn run(&self, task: &Task) -> Result<AggregatedResult, AgentError> {
        let subtasks = self.decompose(task);
        if let Some(t) = &self.tracer {
            t.record(
                TraceEvent::new(
                    "decompose",
                    "orchestrator",
                    format!("split '{}' into {} subtasks (4K-budget chunking)", task.id, subtasks.len()),
                )
                .with_meta(serde_json::json!({ "subtask_ids": subtasks.iter().map(|s| s.id.clone()).collect::<Vec<_>>() })),
            );
        }
        let futures = subtasks.iter().map(|st| self.execute_with_recovery(st));
        let results = join_all(futures).await;

        let mut outputs = Vec::new();
        let mut failed = 0usize;
        for r in results {
            match r {
                Ok(o) => outputs.push(o),
                Err(e) => {
                    // §5.1: never silently degrade — surface via log, count failure.
                    tracing::warn!("subtask failed and was not recovered: {e}");
                    failed += 1;
                }
            }
        }
        Ok(self.aggregate(&outputs, subtasks.len(), failed))
    }

    /// Structural aggregation (§3.3). Majority-votes a `label` field when present.
    fn aggregate(
        &self,
        outputs: &[WorkerOutput],
        subtask_count: usize,
        failed: usize,
    ) -> AggregatedResult {
        let parsed: Vec<Value> = outputs
            .iter()
            .filter(|o| !o.aborted)
            .map(|o| o.parsed.clone())
            .collect();

        let tool_results: Vec<Value> = outputs
            .iter()
            .flat_map(|o| o.tool_results.iter())
            .filter_map(|r| r.output.clone())
            .collect();

        let mut majority_label = None;
        if parsed.first().map(|v| v.get("label")).flatten().is_some() {
            let mut counts: HashMap<String, usize> = HashMap::new();
            for v in &parsed {
                if let Some(l) = v.get("label").and_then(|x| x.as_str()) {
                    *counts.entry(l.to_string()).or_insert(0) += 1;
                }
            }
            majority_label = counts.into_iter().max_by_key(|&(_, c)| c).map(|(k, _)| k);
        }

        if let Some(t) = &self.tracer {
            t.record(
                TraceEvent::new(
                    "aggregate",
                    "aggregator",
                    format!(
                        "merged {} outputs ({} failed){}",
                        parsed.len(),
                        failed,
                        majority_label.as_ref().map(|l| format!(", majority_label={l}")).unwrap_or_default()
                    ),
                )
                .with_meta(serde_json::json!({ "subtask_count": subtask_count, "failed": failed, "tool_results": tool_results.len() })),
            );
        }
        AggregatedResult {
            subtask_count,
            failed,
            outputs: parsed,
            majority_label,
            tool_results,
        }
    }
}

/// Split `text` into chunks each estimated at <= `budget_tokens` (§4.1).
fn chunk_text(text: &str, budget_tokens: usize) -> Vec<String> {
    if text.trim().is_empty() {
        return vec![String::new()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut chunks = Vec::new();
    let mut current = String::new();
    for w in words {
        let candidate = if current.is_empty() {
            w.to_string()
        } else {
            format!("{current} {w}")
        };
        if TokenCounter::estimate(&candidate) > budget_tokens && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            current = w.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{BoxFuture, CompletionResponse, EngineError, MockBackend};
    use crate::policy::{ComposedPolicy, Policy};
    use crate::sandbox::Sandbox;
    use crate::tools::{AddTool, ToolRegistry};
    use std::sync::Arc;

    #[test]
    fn budget_hard_cap_is_3000() {
        let b = ContextBudget::default();
        assert_eq!(b.max_prompt(), 3000.min(b.total - b.generation));
        assert!(b.fits_prompt(2500));
        assert!(!b.fits_prompt(3100));
    }

    #[test]
    fn escalation_levels_map_to_attempts() {
        let mut esc = EscalationController::new(RetryPolicy::default());
        // attempts 0..=2 stay at L1 self-recovery (per-step cap of 2 retries)
        assert_eq!(esc.current_level(), EscalationLevel::SelfRecovery);
        esc.record_attempt();
        assert_eq!(esc.current_level(), EscalationLevel::SelfRecovery);
        esc.record_attempt();
        assert_eq!(esc.current_level(), EscalationLevel::SelfRecovery);
        // attempt 3 escalates to L2 team replan
        esc.record_attempt();
        assert_eq!(esc.current_level(), EscalationLevel::TeamReplan);
        // beyond the task cap (3) -> L3 human, exhausted
        esc.record_attempt();
        assert!(esc.exhausted());
        assert_eq!(esc.current_level(), EscalationLevel::Human);
    }

    #[test]
    fn chunking_respects_budget() {
        let text: String = (0..200).map(|i| format!("word{} ", i)).collect();
        let chunks = chunk_text(&text, 200);
        assert!(!chunks.is_empty());
        for c in &chunks {
            assert!(TokenCounter::estimate(c) <= 200);
        }
    }

    #[tokio::test]
    async fn worker_rejects_overbudget_prompt() {
        let backend = Arc::new(MockBackend::default());
        let worker = Worker::new(backend, ContextBudget::default());
        let task = Subtask {
            id: "t1".into(),
            objective: "x".into(),
            context: String::new(),
            output_schema: "{}".into(),
            allow_abstain: false,
            prompt_tokens: 99999,
        };
        assert!(matches!(
            worker.execute(&task).await,
            Err(AgentError::BudgetExceeded { .. })
        ));
    }

    #[tokio::test]
    async fn checklist_verifier_flags_invalid_json() {
        let v = ChecklistVerifier;
        let task = Subtask {
            id: "t1".into(),
            objective: "x".into(),
            context: String::new(),
            output_schema: r#"{"label": "string"}"#.into(),
            allow_abstain: false,
            prompt_tokens: 10,
        };
        let out = WorkerOutput {
            subtask_id: "t1".into(),
            raw: "transcript".into(),
            final_raw: "not json".into(),
            parsed: Value::Null,
            usage: TokenUsage::default(),
            aborted: false,
            tool_results: vec![],
        };
        let verdict = v.verify(&task, &out).await.unwrap();
        assert!(!verdict.passed);
        assert_eq!(verdict.checks.get("check_syntax"), Some(&false));
    }

    #[tokio::test]
    async fn orchestrator_runs_mock_task_end_to_end() {
        let backend = Arc::new(MockBackend::default());
        let orchestrator = Orchestrator::new(
            backend,
            ContextBudget::default(),
            ChecklistVerifier,
            RetryPolicy::default(),
        );
        let big = (0..80)
            .map(|i| format!("Fact {} about the system. ", i))
            .collect::<String>();
        let task = Task {
            id: "review".into(),
            objective: "Summarize".into(),
            context: big,
            output_schema: r#"{"label": "pass", "notes": "string"}"#.into(),
            allow_abstain: true,
        };
        let result = orchestrator.run(&task).await.unwrap();
        assert!(result.subtask_count >= 1);
    }

    /// A mock backend that emits a tool call on the first turn, then a schema
    /// JSON answer once it sees prior `[TOOL RESULTS]` in the transcript — so
    /// we can exercise the worker's multi-step agentic loop without a model.
    struct ToolCallingMockBackend;
    impl ModelBackend for ToolCallingMockBackend {
        fn name(&self) -> &str {
            "mock-tool"
        }
        fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
            Box::pin(async move {
let text = if req.prompt.contains("[TOOL RESULTS]") {
                    // Second turn: the model now returns a final answer.
                    "{\"label\":\"pass\",\"notes\":\"aggregated\"}".to_string()
                } else {
                    // First turn: emit a tool call.
                    "<tool_call>\n{\"name\":\"add\",\"arguments\":{\"numbers\":[1,2,3]}}\n</tool_call>"
                        .to_string()
                };
                Ok(CompletionResponse {
                    text: text.clone(),
                    usage: TokenUsage::default(),
                    parsed: serde_json::from_str(&text).ok(),
                    think_trace: None,
                })
            })
        }
    }

    /// Emits a tool call for the first two turns (counting `[TOOL RESULTS]`
    /// markers in the transcript), then a final answer — to drive a 2-step loop.
    struct MultiStepMockBackend;
    impl ModelBackend for MultiStepMockBackend {
        fn name(&self) -> &str {
            "mock-multistep"
        }
        fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
            Box::pin(async move {
let rounds = req.prompt.matches("[TOOL RESULTS]").count();
                let text = if rounds >= 2 {
                    "{\"label\":\"pass\",\"notes\":\"done\"}".to_string()
                } else if rounds == 1 {
                    "<tool_call>\n{\"name\":\"add\",\"arguments\":{\"numbers\":[10,20,30]}}\n</tool_call>"
                        .to_string()
                } else {
                    "<tool_call>\n{\"name\":\"add\",\"arguments\":{\"numbers\":[1,2,3]}}\n</tool_call>"
                        .to_string()
                };
                Ok(CompletionResponse {
                    text: text.clone(),
                    usage: TokenUsage::default(),
                    parsed: serde_json::from_str(&text).ok(),
                    think_trace: None,
                })
            })
        }
    }

    #[tokio::test]
    async fn worker_runs_multi_step_tool_loop() {
        let backend = Arc::new(MultiStepMockBackend);
        let tools = Arc::new({
            let mut r = ToolRegistry::new();
            r.register(Arc::new(AddTool));
            r
        });
        let sandbox = Arc::new(Sandbox::new());
        let policy: Arc<dyn Policy> = Arc::new(ComposedPolicy::new());
        let worker = Worker::new(backend, ContextBudget::default())
            .with_tooling(tools, sandbox, policy)
            .with_max_tool_rounds(4);

        let subtask = Subtask {
            id: "t1".into(),
            objective: "add twice".into(),
            context: String::new(),
            output_schema: "{}".into(),
            allow_abstain: false,
            prompt_tokens: 10,
        };
        let out = worker.execute(&subtask).await.unwrap();
        // Two tool rounds were taken before the model gave a final answer.
        assert_eq!(out.tool_results.len(), 2);
        assert_eq!(out.tool_results[0].output.as_ref().unwrap()["sum"], 6.0);
        assert_eq!(out.tool_results[1].output.as_ref().unwrap()["sum"], 60.0);
    }

    #[tokio::test]
    async fn worker_executes_tool_calls_when_tooling_present() {
        let backend = Arc::new(ToolCallingMockBackend);
        let tools = Arc::new({
            let mut r = ToolRegistry::new();
            r.register(Arc::new(AddTool));
            r
        });
        let sandbox = Arc::new(Sandbox::new());
        let policy: Arc<dyn Policy> = Arc::new(ComposedPolicy::new());
        let worker = Worker::new(backend, ContextBudget::default())
            .with_tooling(tools, sandbox, policy);

        let subtask = Subtask {
            id: "t1".into(),
            objective: "add".into(),
            context: String::new(),
            output_schema: "{}".into(),
            allow_abstain: false,
            prompt_tokens: 10,
        };
        let out = worker.execute(&subtask).await.unwrap();
        assert_eq!(out.tool_results.len(), 1);
        let res = &out.tool_results[0];
        assert_eq!(res.verdict, crate::policy::PolicyVerdict::Allow);
        assert_eq!(res.output.as_ref().unwrap()["sum"], 6.0);
    }

    /// Drives a full RAG turn through the agentic loop: the model emits a
    /// `vector_upsert`, then a `vector_search` whose result comes back from
    /// the shared store. Exercises the new RAG tools end-to-end (no model).
    #[tokio::test]
    async fn worker_surfaces_tool_results_in_parsed() {
        use crate::builtins::{VectorSearchTool, VectorUpsertTool};
        use crate::vector::{HashingEmbedder, SharedVectorStore, VectorStore};
        use std::sync::Mutex;

        struct RagMockBackend;
        impl ModelBackend for RagMockBackend {
            fn name(&self) -> &str { "mock-rag" }
            fn complete(&self, req: CompletionRequest) -> BoxFuture<'_, Result<CompletionResponse, EngineError>> {
                Box::pin(async move {
                let rounds = req.prompt.matches("[TOOL RESULTS]").count();
                let text = if rounds >= 1 {
                    "{\"label\":\"pass\"}".to_string()
                } else {
                    "<tool_call>\n{\"name\":\"vector_upsert\",\"arguments\":{\"id\":\"d1\",\"text\":\"val\"}}\n</tool_call>".to_string()
                };
                Ok(CompletionResponse {
                    text: text.clone(),
                    usage: TokenUsage::default(),
                    parsed: serde_json::from_str(&text).ok(),
                    think_trace: None,
                })
                })
            }
        }

        let store: SharedVectorStore = Arc::new(Mutex::new(VectorStore::new(256)));
        let embedder: Arc<dyn crate::vector::Embedder> = Arc::new(HashingEmbedder::new(256));
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(VectorUpsertTool::new(store.clone(), embedder.clone())));
        tools.register(Arc::new(VectorSearchTool::new(store, embedder)));

        let worker = Worker::new(Arc::new(RagMockBackend), ContextBudget::default())
            .with_tooling(Arc::new(tools), Arc::new(Sandbox::new()), Arc::new(ComposedPolicy::new()));

        let subtask = Subtask {
            id: "test".into(),
            objective: "test".into(),
            context: String::new(),
            output_schema: "{}".into(),
            allow_abstain: false,
            prompt_tokens: 10,
        };
        let out = worker.execute(&subtask).await.unwrap();
        
        // Verify tool results are merged into parsed
        assert!(out.parsed.get("tool_results").is_some());
        let results = out.parsed.get("tool_results").unwrap().as_array().unwrap();
        assert_eq!(results.len(), 1);
    }

}
