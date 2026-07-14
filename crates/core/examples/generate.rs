//! Simple one-shot generation CLI.
//!
//! ```bash
//! cargo run -p roco-core --features grammar-rwkv --example generate --release -- \
//!   "The capital of France is"
//! ```
//!
//! Options:
//!   --system <text>    System prompt
//!   --temp <n>         Temperature (0.0–2.0, default 0.7)
//!   --topp <n>         Top-p (0.0–1.0, default 0.9)
//!   --max <n>          Max tokens (default 256)
//!   --grammar <file>   GBNF grammar file
//!   --session <id>     Session ID for stateful continuation

use std::io::{self, Write};

use roco_core::engine::{CompletionRequest, ModelBackend};
use roco_core::rwkv_backend::RwkvBackend;

fn usage() {
    eprintln!(
        "Usage: generate [OPTIONS] <prompt>\n\
         \n\
         Options:\n\
         --system <text>    System prompt\n\
         --temp <n>         Temperature (0.0-2.0, default 0.7)\n\
         --topp <n>         Top-p (0.0-1.0, default 0.9)\n\
         --max <n>          Max tokens (default 256)\n\
         --grammar <file>   GBNF grammar file for constrained output\n\
         --session <id>     Session ID for stateful continuation\n\
         --help             Show this help"
    );
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    let mut system = String::new();
    let mut temperature = 0.7f32;
    let mut top_p = 0.9f32;
    let mut max_tokens = 256usize;
    let mut grammar: Option<String> = None;
    let mut session: Option<String> = None;
    let mut prompt: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--system" => {
                i += 1;
                system = args.get(i).cloned().unwrap_or_default();
            }
            "--temp" => {
                i += 1;
                if let Some(t) = args.get(i).and_then(|v| v.parse::<f32>().ok()) {
                    temperature = t.clamp(0.0, 2.0);
                }
            }
            "--topp" => {
                i += 1;
                if let Some(p) = args.get(i).and_then(|v| v.parse::<f32>().ok()) {
                    top_p = p.clamp(0.0, 1.0);
                }
            }
            "--max" => {
                i += 1;
                if let Some(n) = args.get(i).and_then(|v| v.parse::<usize>().ok()) {
                    max_tokens = n;
                }
            }
            "--grammar" => {
                i += 1;
                if let Some(path) = args.get(i) {
                    match std::fs::read_to_string(path) {
                        Ok(g) => grammar = Some(g),
                        Err(e) => {
                            eprintln!("Failed to read grammar: {e}");
                            std::process::exit(1);
                        }
                    }
                }
            }
            "--session" => {
                i += 1;
                session = args.get(i).cloned();
            }
            "--help" => {
                usage();
                return Ok(());
            }
            arg if arg.starts_with('-') => {
                eprintln!("Unknown option: {arg}");
                usage();
                std::process::exit(1);
            }
            _ => {
                // Collect remaining args as prompt (in case it has spaces)
                prompt = Some(args[i..].join(" "));
                break;
            }
        }
        i += 1;
    }

    let Some(prompt) = prompt else {
        eprintln!("Error: no prompt provided.");
        usage();
        std::process::exit(1);
    };

    let grammar_str = grammar.clone();

    eprintln!("Loading model…");
    let backend = RwkvBackend::from_env()?;

    eprintln!("Generating (temp={temperature}, top_p={top_p}, max={max_tokens})…\n");

    let req = CompletionRequest {
        system,
        prompt,
        output_schema: None,
        grammar: grammar_str,
        temperature,
        max_tokens,
        estimated_prompt_tokens: 0,
        thinking: false,
        preserve_state: session.is_some(),
        on_token: Some(Box::new(move |token: &str| {
            print!("{token}");
            let _ = io::stdout().flush();
        })),
        session,
    };

    match backend.complete(req).await {
        Ok(resp) => {
            println!();
            eprintln!(
                "\n[prompt={} completion={}]",
                resp.usage.prompt_tokens, resp.usage.completion_tokens
            );
        }
        Err(e) => {
            eprintln!("\nError: {e}");
            std::process::exit(1);
        }
    }

    Ok(())
}
