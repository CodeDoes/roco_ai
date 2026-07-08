//! Evaluation harness runner.
//!
//! Runs the eval suite (the `evals/<name>` directories) through the
//! orchestrator using the configured backend. Execution is **sequential** —
//! each eval, and each subtask within it, runs one at a time. This matches the
//! capacity budget (only one NVIDIA-hosted model / one GPU slot) and avoids
//! fan-out that would exceed it.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use crate::agent::{AggregatedResult, Orchestrator, Task, Verifier};
use crate::engine::ModelBackend;

/// Eval suite names — kept in sync with the `evals/` directory tree.
pub const EVAL_NAMES: &[&str] = &[
    "delegate",
    "write_chapter",
    "read_chapter_and_summarize",
    "clear_workspace",
    "delegate_summarize_chapters",
    "chapter_critique",
    "long_message",
    "message_interrupt_resume",
    "tool_calls",
    "skills_load",
    "instruction_follow",
    "policy_follow",
    "sandbox_guard_intercept_handling",
    "can_bypass_loose_sandbox_guard",
    "context_management_outline_delegate_chapter_write_then_wiki",
    "delegate_multi_can_queue_or_parralel_depending_on_model_config",
];

/// Result of running a single eval.
#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    pub name: String,
    pub subtask_count: usize,
    pub failed: usize,
    pub ok: bool,
    pub outputs: Vec<Value>,
}

impl EvalResult {
    fn from_agg(name: &str, agg: &AggregatedResult) -> Self {
        let ok = agg.subtask_count > 0 && agg.failed == 0;
        EvalResult {
            name: name.to_string(),
            subtask_count: agg.subtask_count,
            failed: agg.failed,
            ok,
            outputs: agg.outputs.clone(),
        }
    }
}

/// Build the task for an eval. Each eval is a single atomic task whose context
/// notes its result directory.
fn eval_task(name: &str) -> Task {
    Task {
        id: name.to_string(),
        objective: format!("Execute the '{name}' evaluation and emit a JSON verdict."),
        context: format!("Eval suite: {name}. Persist findings under evals/{name}/."),
        // Simple schema so the (NVIDIA + JSON-mode) model returns valid JSON
        // and the checklist verifier passes.
        output_schema: r#"{"status": "<pass|fail>", "notes": "<string>"}"#.into(),
        allow_abstain: true,
    }
}

/// Run one eval through the orchestrator (sequentially) and write
/// `evals/<name>/result.json`.
pub async fn run_eval<B, V>(orch: &Orchestrator<B, V>, name: &str) -> Result<EvalResult>
where
    B: ModelBackend + Send + Sync,
    V: Verifier,
{
    let task = eval_task(name);
    let agg = orch.run_sequential(&task).await?;
    let result = EvalResult::from_agg(name, &agg);
    let path = Path::new("evals").join(name).join("result.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&result)?);
    Ok(result)
}

/// Run all evals **sequentially**, returning a result per eval.
pub async fn run_all_evals<B, V>(orch: &Orchestrator<B, V>) -> Result<Vec<EvalResult>>
where
    B: ModelBackend + Send + Sync,
    V: Verifier,
{
    let mut results = Vec::new();
    for name in EVAL_NAMES {
        // Await each eval before starting the next — strictly sequential.
        let r = run_eval(orch, name).await?;
        results.push(r);
    }
    Ok(results)
}
