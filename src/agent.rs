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

use futures::future::join_all;
use serde_json::Value;
use thiserror::Error;

use crate::engine::{CompletionRequest, ModelBackend, TokenCounter, TokenUsage};

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
    pub raw: String,
    pub parsed: Value,
    pub usage: TokenUsage,
    /// True when the worker chose the do_nothing / abstain action.
    pub aborted: bool,
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
pub struct Worker<B: ModelBackend + Send + Sync> {
    backend: Arc<B>,
    budget: ContextBudget,
}

impl<B: ModelBackend + Send + Sync> Worker<B> {
    pub fn new(backend: Arc<B>, budget: ContextBudget) -> Self {
        Self { backend, budget }
    }

    /// Execute a single atomic subtask with a schema-first prompt and sampling
    /// discipline (§2.2). Enforces the context budget before dispatch.
    pub async fn execute(&self, task: &Subtask) -> Result<WorkerOutput, AgentError> {
        if !self.budget.fits_prompt(task.prompt_tokens) {
            return Err(AgentError::BudgetExceeded {
                id: task.id.clone(),
                used: task.prompt_tokens,
                max: self.budget.max_prompt(),
            });
        }
        let system = build_system_prompt(&task.output_schema, task.allow_abstain);
        let req = CompletionRequest {
            system,
            prompt: task.render_prompt(),
            output_schema: Some(task.output_schema.clone()),
            temperature: 0.2,
            max_tokens: 512,
            estimated_prompt_tokens: task.prompt_tokens,
        };
        let resp = self.backend.complete(req).await?;
        let parsed = serde_json::from_str::<Value>(&resp.text).unwrap_or(Value::Null);
        let aborted = task.allow_abstain
            && parsed.get("action").and_then(|a| a.as_str()) == Some("do_nothing");
        Ok(WorkerOutput {
            subtask_id: task.id.clone(),
            raw: resp.text,
            parsed,
            usage: resp.usage,
            aborted,
        })
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

impl Verifier for ChecklistVerifier {
    async fn verify(
        &self,
        task: &Subtask,
        output: &WorkerOutput,
    ) -> Result<VerificationVerdict, AgentError> {
        let mut checks: HashMap<String, bool> = HashMap::new();

        let parsed = match serde_json::from_str::<Value>(&output.raw) {
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
            task.objective, output.raw
        );
        let est = TokenCounter::estimate(&prompt);
        let req = CompletionRequest {
            system: system.into(),
            prompt,
            output_schema: None,
            temperature: 0.1,
            max_tokens: 256,
            estimated_prompt_tokens: est,
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
}

/// Orchestrator-Worker controller (§1.1). Decomposes a task, fans out to workers,
/// gates each result through a [`Verifier`], and aggregates.
pub struct Orchestrator<B: ModelBackend + Send + Sync, V: Verifier> {
    backend: Arc<B>,
    budget: ContextBudget,
    verifier: V,
    retry_policy: RetryPolicy,
}

impl<B: ModelBackend + Send + Sync, V: Verifier> Orchestrator<B, V> {
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
        }
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
        let worker = Worker::new(Arc::clone(&self.backend), self.budget.clone());
        let mut esc = EscalationController::new(self.retry_policy.clone());
        loop {
            match worker.execute(task).await {
                Ok(output) => {
                    let verdict = self.verifier.verify(task, &output).await?;
                    if verdict.passed {
                        return Ok(output);
                    }
                    esc.record_attempt();
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

        AggregatedResult {
            subtask_count,
            failed,
            outputs: parsed,
            majority_label,
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
    use crate::engine::MockBackend;

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
            raw: "not json".into(),
            parsed: Value::Null,
            usage: TokenUsage::default(),
            aborted: false,
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
}
