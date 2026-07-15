//! Folder-bound agent session example.
//!
//! Opens (or initializes) a persistent agent session rooted at a project
//! folder: long-term memory and searchable session history are stored under
//! `<folder>/.roco/agent_chat/` and the agent's workspace is the folder itself,
//! so it can read and edit the project across invocations.
//!
//! Usage:
//!   cargo run -p roco-cli --example agent_chat --release -- <folder> [task...]

use std::path::PathBuf;

use roco_agent::AgentChatSession;
use roco_engine::ModelBackend;
use roco_inference::RwkvBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (folder, task) = match args.split_first() {
        Some((folder, rest)) => (
            PathBuf::from(folder),
            rest.join(" "),
        ),
        None => {
            eprintln!("Usage: agent_chat <folder> [task...]");
            std::process::exit(2);
        }
    };
    let task = if task.trim().is_empty() {
        "Summarize what you can do in this folder and note anything worth remembering.".to_string()
    } else {
        task
    };

    println!("Agent session folder: {}\n", folder.display());
    eprintln!("Opening persistent session...\n");
    let chat = AgentChatSession::open(&folder)?;
    eprintln!(
        "Memory entries: {}, past sessions: {}\n",
        chat.memory.len(),
        chat.sessions.len()
    );

    eprintln!("Loading model...\n");
    let backend = RwkvBackend::from_env()?;
    eprintln!("Backend: {} ready.\n", backend.name());

    eprintln!("Running agent loop...\n");
    let trace = chat.run(&backend, &task).await?;

    // Run any scheduled tasks that are now due.
    let due = chat.scheduler.run_due(&backend).await?;
    if !due.is_empty() {
        eprintln!("Ran {} scheduled task(s).\n", due.len());
    }
    chat.persist()?;

    println!("\n=== Agent Result ===\n");
    if trace.completed {
        println!("{}", trace.final_text);
    } else {
        println!("(incomplete after {} steps)\n{}", trace.steps.len(), trace.final_text);
    }
    println!(
        "\nSession persisted under {}/.roco/agent_chat/ ({} memory entries, {} past sessions).\n",
        folder.display(),
        chat.memory.len(),
        chat.sessions.len()
    );
    Ok(())
}
