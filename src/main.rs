#![allow(dead_code)]
// Foundation scaffold: the orchestration API (tool/RAG budget fields, the
// `JudgeVerifier` variant, constrained-decoding hooks, per-output observability
// fields) is intentionally built ahead of its consumers (tools.rs, grammar.rs,
// a real ModelBackend). These will be exercised as the foundation grows.

//! RoCo AI — foundation smoke test.
//!
//! Wires the orchestration layer (`agent`) to the [`MockBackend`] so the full
//! Orchestrator-Worker pipeline runs end-to-end *before* a real 3B model is
//! downloaded. Swap `MockBackend` for a `ModelBackend` implementation later.

mod agent;
mod engine;
mod capacity;
mod config;
mod tools;
mod grammar;
#[cfg(feature = "http-backends")]
mod backends;

use std::sync::Arc;

use agent::{ContextBudget, Orchestrator, RetryPolicy, Task, ChecklistVerifier};
use engine::MockBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let backend = Arc::new(MockBackend {
        name: "mock-3b".into(),
        ..Default::default()
    });
    let budget = ContextBudget::default();

    println!("RoCo AI — foundation smoke test");
    println!(
        "Context budget (4K): total={}  max_prompt={}  task_context={}",
        budget.total, budget.max_prompt(), budget.task_context
    );

    let orchestrator = Orchestrator::new(backend, budget, ChecklistVerifier, RetryPolicy::default());

    // --- Demo A: happy path + budget-aware chunking (fan-out) --------------
    // Large context forces decomposition into several atomic subtasks. The
    // schema matches the mock's output shape, so the verification gate passes.
    let context_a = (0..120)
        .map(|i| format!("Fact {}: the orchestrator routes subtask {} through a verification gate. ", i, i))
        .collect::<Vec<_>>()
        .join("");
    let task_a = Task {
        id: "doc-review".into(),
        objective: "Review the provided facts.".into(),
        context: context_a,
        // matches the mock backend's `{"result": ...}` output
        output_schema: r#"{"result": "<string>"}"#.into(),
        allow_abstain: true,
    };
    println!("\n=== Demo A: decomposition + passing verification gate ===");
    let result_a = orchestrator.run(&task_a).await?;
    println!("Subtasks executed : {}", result_a.subtask_count);
    println!("Failed subtasks   : {}", result_a.failed);
    println!("Majority label    : {:?}", result_a.majority_label);

    // --- Demo B: mismatch triggers verification failure + escalation ------
    // The schema does NOT match the mock output, so the gate fails and the
    // retry/escalation cascade runs up to human intervention (§5.1, §5.3).
    let task_b = Task {
        id: "triage".into(),
        objective: "Classify the request.".into(),
        context: "Schedule a meeting for Thursday.".into(),
        output_schema: r#"{"label": "<pass|fail>", "notes": "<string>"}"#.into(),
        allow_abstain: true,
    };
    println!("\n=== Demo B: verification failure -> escalation cascade ===");
    let result_b = orchestrator.run(&task_b).await?;
    println!("Subtasks executed : {}", result_b.subtask_count);
    println!("Failed subtasks   : {}", result_b.failed);
    println!(
        "(Expected: failed > 0 because the mock output does not satisfy the schema, \n\n  exercising the retry circuit breaker and L3 human-intervention path.)"
    );

    // Real HTTP backends (only compiled with --features http-backends).
    #[cfg(feature = "http-backends")]
    demo_real_backends().await?;

    Ok(())
}

/// Demonstrates swapping the mock for a real provider selected by config
/// (defaults to NVIDIA). Runs only when the relevant API key is present.
#[cfg(feature = "http-backends")]
async fn demo_real_backends() -> anyhow::Result<()> {
    use std::sync::Arc;

    use crate::agent::{Orchestrator, Task, ChecklistVerifier};
    use crate::backends::AnyBackend;
    use crate::config::Config;

    // Load API keys from a local .env file (e.g. NVIDIA_API_KEY, KILO_API_KEY).
    let _ = dotenvy::dotenv();

    let cfg = Config::load_or_preset("model/default_config");
    println!("\n=== Demo: config-driven backend (provider={:?}) ===", cfg.provider);

    let backend: AnyBackend = match cfg.build_backend() {
        Ok(b) => b,
        Err(e) => {
            println!("(skip: could not build backend: {e})");
            return Ok(());
        }
    };

    let orch = Orchestrator::new(
        Arc::new(backend),
        cfg.context_budget(),
        ChecklistVerifier,
        cfg.retry_policy(),
    );
    let task = Task {
        id: "live-smoke".into(),
        objective: "Reply with a JSON object: {\"ok\": true}.".into(),
        context: String::new(),
        output_schema: r#"{"ok": "<bool>"}"#.into(),
        allow_abstain: false,
    };
    match orch.run(&task).await {
        Ok(r) => println!("subtasks: {}  failed: {}", r.subtask_count, r.failed),
        Err(e) => println!("run error: {e}"),
    }
    Ok(())
}
