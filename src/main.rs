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
mod sandbox;
mod policy;
mod toolcall;
mod builtins;
mod infer;
mod vector;
mod audio;
mod eval;
#[cfg(feature = "http-backends")]
mod backends;

use std::io::Write;
use std::sync::{Arc, Mutex};

use agent::{ContextBudget, Orchestrator, RetryPolicy, Task, ChecklistVerifier};
use crate::sandbox::Sandbox;
use engine::MockBackend;
use tracing_subscriber::fmt::writer::MakeWriter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing()?;

    // `eval` runs a single suite through the NVIDIA endpoint only (no other
    // providers). Only available with the http-backends feature compiled in.
    #[cfg(feature = "http-backends")]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).map(String::as_str) == Some("eval") {
            return run_eval_cli(&args[2..]).await;
        }
    }

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

    // --- Demo C: RAG toolkit (vector embed + search) + audio tool stubs -----
    // Builds the full agent toolkit (files + RAG + STT/TTS) and runs a
    // vector_upsert -> vector_search round-trip entirely locally (no model).
    println!("\n=== Demo C: RAG vector store (embed + search) ===");
    let root = std::env::temp_dir().join("roco-demo");
    let _ = std::fs::create_dir_all(&root);
    let toolkit = builtins::default_agent_toolkit(root.clone(), Sandbox::new());
    println!("Toolkit tools: {}", toolkit.schemas_json().as_array().unwrap().len());
    let _ = toolkit
        .dispatch(
            "vector_upsert",
            serde_json::json!({ "id": "doc1", "text": "RoCo is a small, fast, stateful agent" }),
        )
        .await?;
    let search = toolkit
        .dispatch(
            "vector_search",
            serde_json::json!({ "query": "small fast agent", "k": 1 }),
        )
        .await?;
    let top = &search["hits"][0];
    println!("top hit: id={} score={:.3}", top["id"], top["score"].as_f64().unwrap());

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

/// Runs a single eval suite through the NVIDIA endpoint only.
///
/// Usage: `cargo run --features http-backends -- eval [NAME]`
/// `NAME` defaults to the first suite in [`crate::eval::EVAL_NAMES`]. The
/// NVIDIA backend is built directly from the environment (NVIDIA_API_KEY /
/// NV_MODEL via a local `.env`), so no other provider is ever contacted.
#[cfg(feature = "http-backends")]
async fn run_eval_cli(rest: &[String]) -> anyhow::Result<()> {
    use std::sync::Arc;

    use crate::agent::{Orchestrator, ChecklistVerifier};
    use crate::backends::NvidiaBackend;
    use crate::config::Config;
    use crate::eval::{EVAL_NAMES, run_eval};

    // Load NVIDIA_API_KEY / NV_MODEL from a local .env if present.
    let _ = dotenvy::dotenv();

    // Resolve the model that will actually be used (mirrors NvidiaBackend).
    let model = std::env::var("NV_MODEL")
        .unwrap_or_else(|_| crate::backends::NvidiaBackend::DEFAULT_MODEL.to_string());
    tracing::info!(model = %model, "nvidia eval backend");
    println!("NVIDIA model: {model}");

    let name = rest
        .first()
        .cloned()
        .unwrap_or_else(|| EVAL_NAMES[0].to_string());
    if !EVAL_NAMES.contains(&name.as_str()) {
        anyhow::bail!(
            "unknown eval '{name}'.\nvalid evals:\n  {}",
            EVAL_NAMES.join("\n  ")
        );
    }

    let cfg = Config::preset();
    let backend = Arc::new(NvidiaBackend::from_env()?);
    let orch = Orchestrator::new(
        backend,
        cfg.context_budget(),
        ChecklistVerifier,
        cfg.retry_policy(),
    );

    println!("Running eval '{name}' via NVIDIA endpoint only.");
    let result = run_eval(&orch, &name).await?;
    println!(
        "Eval '{name}': ok={}  subtasks={}  failed={}",
        result.ok, result.subtask_count, result.failed
    );
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

/// Initialize tracing to emit to BOTH the console and a file under the
/// artifact root `.roco/logs/roco.log`, so runs are never blind. The default
/// filter is `info` globally with `roco_ai=debug` (backend request/response
/// visibility); override via the `RUST_LOG` env var.
fn init_tracing() -> anyhow::Result<()> {
    let _ = std::fs::create_dir_all(".roco/logs");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(".roco/logs/roco.log")?;
    let sinks = Arc::new(vec![
        Mutex::new(Box::new(std::io::stdout()) as Box<dyn Write + Send>),
        Mutex::new(Box::new(file) as Box<dyn Write + Send>),
    ]);
    let writer = TeeWriter { sinks };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,roco_ai=debug"));
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_env_filter(filter)
        .with_target(false)
        .with_timer(tracing_subscriber::fmt::time::SystemTime)
        .init();
    Ok(())
}

/// A `MakeWriter` that fans every log line out to multiple sinks (console + file).
struct TeeWriter {
    sinks: Arc<Vec<Mutex<Box<dyn Write + Send>>>>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for sink in self.sinks.iter() {
            sink.lock().unwrap().write_all(buf)?;
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        for sink in self.sinks.iter() {
            sink.lock().unwrap().flush()?;
        }
        Ok(())
    }
}

impl Clone for TeeWriter {
    fn clone(&self) -> Self {
        TeeWriter {
            sinks: Arc::clone(&self.sinks),
        }
    }
}

impl<'a> MakeWriter<'a> for TeeWriter {
    type Writer = TeeWriter;
    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}
