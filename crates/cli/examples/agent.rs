//! Autonomous agent example: runs a ReAct loop against the local RWKV model.

use roco_agent::{Agent, AgentConfig};
use roco_engine::ModelBackend;
use roco_inference::RwkvBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")))
        .init();

    let task = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "List the files in the current directory and tell me how many there are.".to_string());

    println!("Agent task: {task}\n");
    eprintln!("Loading model...\n");
    let backend = RwkvBackend::from_env()?;
    eprintln!("Backend: {} ready.\n", backend.name());

    let config = AgentConfig {
        enable_tools: true,
        enable_think: true,
        verbose: true,
        ..Default::default()
    };
    let agent = Agent::new(config);

    eprintln!("Running agent loop...\n");
    let trace = agent.run(&backend, &task).await?;

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
