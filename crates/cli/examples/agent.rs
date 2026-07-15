//! Autonomous agent example: runs a ReAct loop against the local RWKV model,
//!
//! The agent is wired with the built-in tools plus a sandboxed `Workspace`,
//! long-term `MemoryStore`, `SessionStore` (searchable history), and a
//! `Scheduler` (deferred/periodic tasks) — so a real run can `read`/`write`
//! inside its workspace, `remember`/`recall` facts, `search_sessions` for
//! prior context, and `schedule` work for later.

use std::sync::Arc;

use roco_agent::{
    Agent, AgentConfig,
    memory::MemoryStore,
    scheduler::Scheduler,
    sessions::SessionStore,
    workspace::{Workspace, WorkspaceKind},
};
use roco_engine::ModelBackend;
use roco_inference::RwkvBackend;
use roco_tools::all_tools;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let task = std::env::args().nth(1).unwrap_or_else(|| {
        "List the files in the current directory and tell me how many there are.".to_string()
    });

    println!("Agent task: {task}\n");
    eprintln!("Loading model...\n");
    let backend = RwkvBackend::from_env()?;
    eprintln!("Backend: {} ready.\n", backend.name());

    // Sandbox + long-term memory + session search + scheduler, combined with
    // the built-in tools into one agent.
    let workspace = Arc::new(Workspace::temp(WorkspaceKind::Agent)?);
    let memory = Arc::new(MemoryStore::new());
    let sessions = Arc::new(SessionStore::new());
    let scheduler = Arc::new(Scheduler::new());

    let mut tools = all_tools();
    tools.extend(Workspace::scoped_tools(workspace.clone()));
    tools.extend(MemoryStore::scoped_tools(memory.clone()));
    tools.extend(SessionStore::scoped_tools(sessions.clone()));
    tools.extend(Scheduler::scoped_tools(scheduler.clone()));

    let config = AgentConfig {
        enable_tools: true,
        enable_think: true,
        verbose: true,
        ..Default::default()
    };
    let agent = Agent::with_tools(config, tools);

    eprintln!("Running agent loop...\n");
    let trace = agent.run(&backend, &task).await?;

    // Record this run so it can be searched later via `search_sessions`.
    sessions.record_trace("last-run", &task, &trace);

    // Run any scheduled tasks that are now due.
    let due = scheduler.run_due(&backend).await?;
    if !due.is_empty() {
        eprintln!("Ran {} scheduled task(s).\n", due.len());
    }

    println!("\n=== Agent Result ===\n");
    if trace.completed {
        println!("{}", trace.final_text);
    } else {
        println!("(run did not complete: {:?})", trace.stop_reason);
        if let Some(last) = trace.steps.last() {
            println!("\nLast assistant output:\n{}", last.assistant_text);
        }
    }

    println!("\n=== Trace ({}) ===", trace.steps.len());
    for step in &trace.steps {
        println!(
            "  step {}: {} tool calls, {} completion tokens",
            step.step,
            step.tool_calls.len(),
            step.usage.completion_tokens
        );
        for call in &step.tool_calls {
            println!("    → {}({})", call.name, call.arguments);
        }
    }
    println!(
        "\nTotal: {} prompt + {} completion tokens",
        trace.total_usage.prompt_tokens, trace.total_usage.completion_tokens
    );

    Ok(())
}
