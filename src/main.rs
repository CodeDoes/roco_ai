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

use std::io::Write;
use std::sync::{Arc, Mutex};

use roco_ai::agent::{ContextBudget, Orchestrator, RetryPolicy, Task, ChecklistVerifier};
use roco_ai::sandbox::Sandbox;
use roco_ai::engine::{CompletionRequest, CompletionResponse, EngineError, ModelBackend, MockBackend, TokenUsage};
use tracing_subscriber::fmt::writer::MakeWriter;

/// Tiny demo backend that returns canned JSON for the memory prompts, so Demo
/// D can show Honcho state synthesis and Zep triple extraction live (the real
/// RWKV7-g1g backend replaces this once wired in).
struct DemoMemoryBackend;
impl ModelBackend for DemoMemoryBackend {
    fn name(&self) -> &str {
        "demo-memory"
    }
    fn complete(
        &self,
        req: CompletionRequest,
    ) -> futures::future::BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        let text = if req.system.contains("dialectic") {
            "{\"identity\":\"Sam\",\"current_goals\":\"ship the memory layer\",\"preferences\":\"concise answers\",\"open_loops\":\"\"}".to_string()
        } else if req.system.contains("subject-predicate-object") {
            serde_json::json!([["User", "lives_in", "Austin"]]).to_string()
        } else if req.system.contains("conflict resolver") {
            "{\"action\":\"UPDATE\",\"target_id\":\"f0\"}".to_string()
        } else {
            "[\"User moved to Austin, TX\"]".to_string()
        };
        Box::pin(async move {
            Ok(CompletionResponse {
                text: text.clone(),
                usage: TokenUsage::default(),
                parsed: serde_json::from_str(&text).ok(),
            })
        })
    }
}

/// A mock backend that emits a tool call on the first turn, then a schema
/// JSON answer once it sees prior `[TOOL RESULTS]` in the transcript.
struct ToolCallingMockBackend;
impl ModelBackend for ToolCallingMockBackend {
    fn name(&self) -> &str {
        "mock-tool"
    }
    fn complete(
        &self,
        req: CompletionRequest,
    ) -> futures::future::BoxFuture<'_, Result<CompletionResponse, EngineError>> {
        let prompt = req.prompt.clone();
        Box::pin(async move {
            let text = if prompt.contains("[TOOL RESULTS]") {
                "{\"label\":\"pass\",\"notes\":\"I successfully used the tool!\"}".to_string()
            } else {
                "<tool_call>\n{\"name\":\"read\",\"arguments\":{\"path\":\"hello.txt\"}}\n</tool_call>".to_string()
            };
            Ok(CompletionResponse {
                text: text.clone(),
                usage: TokenUsage::default(),
                parsed: serde_json::from_str(&text).ok(),
            })
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing()?;

    // `viz` emits a structured execution trace + HTML preview from a real
    // (mock-backed) orchestration run. See `run_viz` below.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("viz") {
        return run_viz().await;
    }

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

    let orchestrator = Orchestrator::new(backend, budget.clone(), ChecklistVerifier, RetryPolicy::default());

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
    let root = std::path::PathBuf::from(".roco/workspaces/temp-demo");
    let _ = std::fs::create_dir_all(&root);
    let toolkit = roco_ai::builtins::default_agent_toolkit(root.clone(), Sandbox::new());
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

    // --- Demo D: RNN memory processor (Mem0 + Honcho + Letta + Zep) --------
    println!("\n=== Demo D: memory processor (extract / resolve / state / graph) ===");
    let mem_embedder: Arc<dyn roco_ai::vector::Embedder> =
        Arc::new(roco_ai::vector::HashingEmbedder::new(256));
    let proc = Arc::new(roco_ai::memory::MemoryProcessor::new(mem_embedder));
    let mb: Arc<DemoMemoryBackend> = Arc::new(DemoMemoryBackend);
    // Mem0 ingest (deterministic fallback: no model needed for the demo).
    proc.ingest_deterministic(
        "sam",
        "I moved to Austin, TX and I love it. My dog is Barnaby.",
    );
    proc.ingest_deterministic("sam", "Now I live in Austin, TX near the lake.");
    let facts = proc.retrieve("sam", "Austin", 5);
    println!("sam facts (semantic): {}", facts.len());
    // Honcho dialectic user-state synthesis.
    let state = proc
        .synthesize_state(mb.as_ref(), "sam", "Sam is building an agent in Rust.")
        .await?;
    println!("sam state identity: {}", state.identity);
    // Zep temporal graph extraction + merge.
    let edges = proc.extract_triples(mb.as_ref(), "Sam lives in Austin.").await?;
    proc.merge_triples(edges);
    println!("graph current 'User' edges: {}", proc.graph().current_for("User").len());
    // Wire the four memory tools into a registry and dispatch one.
    let mut reg = roco_ai::builtins::default_agent_toolkit(root.clone(), Sandbox::new());
    for t in roco_ai::memory::memory_tools(proc.clone(), mb.clone()) {
        reg.register(t);
    }
    let search = reg
        .dispatch(
            "memory_search",
            serde_json::json!({ "user_id": "sam", "query": "Austin", "k": 3 }),
        )
        .await?;
    println!(
        "memory_search tool hits: {}",
        search["facts"].as_array().unwrap().len()
    );

    // --- Demo E: agentic tool-use loop (transcript + merged JSON) -------
    println!("\n=== Demo E: agentic tool-use loop (transcript + merged JSON) ===");
    let tool_backend = Arc::new(ToolCallingMockBackend);
    let mut tool_reg = roco_ai::builtins::default_agent_toolkit(root.clone(), Sandbox::new());
    // Ensure 'read' is in there (it comes from default_agent_toolkit).
    let worker = roco_ai::agent::Worker::new(tool_backend, budget.clone())
        .with_tooling(
            Arc::new(tool_reg),
            Arc::new(Sandbox::new()),
            Arc::new(roco_ai::policy::ComposedPolicy::new()),
        );
    let subtask = roco_ai::agent::Subtask {
        id: "tool-demo".into(),
        objective: "Read hello.txt and tell me what's in it.".into(),
        context: String::new(),
        output_schema: r#"{"label": "pass", "notes": "string"}"#.into(),
        allow_abstain: false,
        prompt_tokens: 10,
    };
    let out = worker.execute(&subtask).await?;
    println!("--- FULL TRANSCRIPT ---\n{}", out.raw);
    println!("\n--- FINAL ANSWER ---\n{}", out.final_raw);
    println!("\n--- MERGED JSON (parsed) ---\n{}", serde_json::to_string_pretty(&out.parsed).unwrap());

    // --- Demo F: High-level DX API (Workspace -> Engine -> Logger) -------
    println!("\n=== Demo F: High-level DX API ===");
    let mut files = std::collections::HashMap::new();
    files.insert("readme.txt".to_string(), "This is a DX demo workspace".to_string());
    
    let ws = roco_ai::workspace::Workspace::temp("dx-demo", &files).unwrap();
    let logger = roco_ai::logger::Logger::new(ws.add_folder("logs"));
    
    let engine = roco_ai::session::Engine::new(
        Arc::new(MockBackend::default()), 
        ws.clone()
    );
    
    engine.queue_message("user", "Tell me a joke about Rust.");
    
    // Run a poll step
    engine.poll().await?;
    
    // Log results
    logger.jsonl("interaction.jsonl", "messages", &engine.messages()).unwrap();
    logger.stream("interaction.txt", "raw", &engine.stream()).unwrap();
    logger.log("interaction.log", "events", &engine.events().join("\n")).unwrap();
    
    println!("DX Demo completed. Logs written to: {}", ws.root.display());
    
    // Real HTTP backends (only compiled with --features http-backends).
    #[cfg(feature = "http-backends")]
    demo_real_backends().await?;

    Ok(())
}

/// Emits a rustviz-style execution trace of a real orchestration run.
///
/// Runs a multi-part task through the `MockBackend` while an attached
/// [`CollectingTracer`] records every architectural step. Writes two artifacts
/// under `.roco/traces/`:
///   * `roco_trace.html` — the current HTML preview (chat + events + graph)
///   * `roco_trace.json` — the durable structured trace (consume this from the
///     web frontend built later)
///
/// Usage: `cargo run -- viz`
async fn run_viz() -> anyhow::Result<()> {
    use std::sync::Arc;

    use roco_ai::agent::{ChecklistVerifier, ContextBudget, Orchestrator, RetryPolicy, Task};
    use roco_ai::engine::MockBackend;
    use roco_ai::trace::CollectingTracer;
    use roco_ai::visualizer::Visualizer;

    let _ = std::fs::create_dir_all(".roco/traces");

    let backend = Arc::new(MockBackend {
        name: "mock-3b".into(),
        ..Default::default()
    });
    let budget = ContextBudget::default();
    let tracer = CollectingTracer::new();

    let orchestrator = Orchestrator::new(
        backend,
        budget.clone(),
        ChecklistVerifier,
        RetryPolicy::default(),
    )
    .with_tracer(Arc::new(tracer.clone()));

    // Multi-part context forces decomposition into several atomic subtasks,
    // exercising the fan-out + verify + aggregate path end-to-end.
    let context: String = (0..400)
        .map(|i| format!("Fact {}: the orchestrator routes subtask {} through a verification gate. ", i, i))
        .collect();
    let task = Task {
        id: "doc-review".into(),
        objective: "Review the provided facts and summarize.".into(),
        context,
        // matches the mock backend's `{"result": ...}` output so the gate passes
        output_schema: r#"{"result": "<string>"}"#.into(),
        allow_abstain: true,
    };

    let subs = orchestrator.decompose(&task);
    let result = orchestrator.run(&task).await?;

    // Conversation view (user objective + assistant summary).
    let messages = serde_json::json!([
        {
            "role": "user",
            "content": format!(
                "Objective: {}\n\n(Context chunked into {} atomic 4K subtasks)",
                task.objective, subs.len()
            )
        },
        {
            "role": "assistant",
            "content": format!(
                "Aggregated {} subtask outputs ({} failed). Majority label: {:?}.",
                result.subtask_count, result.failed, result.majority_label
            )
        }
    ]);

    // Knowledge graph: orchestrator topology + per-worker edges.
    let mut graph: Vec<serde_json::Value> = vec![serde_json::json!([
        "orchestrator", "decomposed_into", format!("{} subtasks", subs.len())
    ])];
    for s in &subs {
        graph.push(serde_json::json!(["orchestrator", "spawned", format!("worker-{}", s.id)]));
        graph.push(serde_json::json!([format!("worker-{}", s.id), "used_backend", "mock-3b"]));
        graph.push(serde_json::json!([format!("worker-{}", s.id), "produced", s.id]));
    }
    let memory = serde_json::Value::Array(graph);

    let trace = tracer.snapshot();
    let html_path = std::path::Path::new(".roco/traces/roco_trace.html");
    let json_path = std::path::Path::new(".roco/traces/roco_trace.json");
    Visualizer::render_trace(&trace, &messages, &memory, html_path)?;
    Visualizer::write_json(&trace, &messages, &memory, json_path)?;

    println!("RoCo AI — visualizer trace");
    println!("  subtasks executed : {}", result.subtask_count);
    println!("  failed subtasks   : {}", result.failed);
    println!("  trace events      : {}", trace.len());
    println!("  HTML  -> {}", html_path.display());
    println!(
        "  JSON  -> {}  (consume this from your future frontend)",
        json_path.display()
    );
    Ok(())
}

/// Demonstrates swapping the mock for a real provider selected by config
/// (defaults to NVIDIA). Runs only when the relevant API key is present.
#[cfg(feature = "http-backends")]
async fn demo_real_backends() -> anyhow::Result<()> {
    use std::sync::Arc;

    use roco_ai::agent::{Orchestrator, Task, ChecklistVerifier};
    use roco_ai::backends::AnyBackend;
    use roco_ai::config::Config;

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
/// `NAME` defaults to the first suite in [`roco_ai::eval::EVAL_NAMES`]. The
/// NVIDIA backend is built directly from the environment (NVIDIA_API_KEY /
/// NV_MODEL via a local `.env`), so no other provider is ever contacted.
#[cfg(feature = "http-backends")]
async fn run_eval_cli(rest: &[String]) -> anyhow::Result<()> {
    use std::sync::Arc;

    use roco_ai::agent::{Orchestrator, ChecklistVerifier};
    use roco_ai::backends::NvidiaBackend;
    use roco_ai::config::Config;
    use roco_ai::eval::{EVAL_NAMES, run_eval};

    // Load NVIDIA_API_KEY / NV_MODEL from a local .env if present.
    let _ = dotenvy::dotenv();

    // Resolve the model that will actually be used (mirrors NvidiaBackend).
    let model = std::env::var("NV_MODEL")
        .unwrap_or_else(|_| roco_ai::backends::NvidiaBackend::DEFAULT_MODEL.to_string());
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
