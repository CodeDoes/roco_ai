//! Interactive chat REPL for the local RWKV model.
//!
//! ```bash
//! cargo run -p roco-core --features grammar-rwkv --example chat --release
//! ```
//!
//! Commands:
//!   /quit or /exit   Leave the chat
//!   /clear           Reset conversation to blank state
//!   /temp <n>        Set sampling temperature (0.0-2.0)
//!   /max <n>         Set max tokens per response
//!   /grammar <path>  Load a GBNF grammar file for constrained output
//!   /grammar off     Disable grammar constraints
//!   /help            Show this help
//!
//! Ctrl+C during generation cancels the current response.

use std::io::{self, BufRead, Write};

use roco_core::engine::{CompletionRequest, ModelBackend};
use roco_core::rwkv_backend::RwkvBackend;

const SESSION: &str = "chat";

fn print_banner() {
    eprintln!();
    eprintln!("  RoCo Chat -- local RWKV-7");
    eprintln!("  Type /help for commands. Ctrl+C to interrupt generation.");
    eprintln!();
}

fn print_help() {
    eprintln!();
    eprintln!("  /quit or /exit   Leave the chat");
    eprintln!("  /clear           Reset conversation to blank state");
    eprintln!("  /temp <n>        Set sampling temperature (0.0-2.0)");
    eprintln!("  /max <n>         Set max tokens per response");
    eprintln!("  /grammar <path>  Load a GBNF grammar file");
    eprintln!("  /grammar off     Disable grammar constraints");
    eprintln!("  /stats           Show token counts and session info");
    eprintln!("  /help            Show this help");
    eprintln!();
}

fn read_line() -> io::Result<String> {
    let stdin = io::stdin();
    let mut line = String::new();
    stdin.lock().read_line(&mut line)?;
    Ok(line.trim_end().to_string())
}

fn do_prompt() -> io::Result<String> {
    print!("> ");
    io::stdout().flush()?;
    read_line()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let mut temperature = 0.5f32;
    let mut max_tokens = 512usize;
    let mut top_p = 0.85f32;
    let mut grammar: Option<String> = None;
    let mut total_prompt = 0u64;
    let mut total_completion = 0u64;

    print_banner();

    eprintln!("Loading model... (this takes a moment)");
    let backend = RwkvBackend::from_env()?;
    eprintln!("Backend: {} -- ready.\n", backend.name());

    loop {
        let input = match do_prompt() {
            Ok(line) => line,
            Err(e) => {
                eprintln!("Input error: {e}");
                break;
            }
        };

        // Handle commands.
        if input.starts_with('/') {
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            match parts[0] {
                "/quit" | "/exit" => break,
                "/help" => print_help(),
                "/clear" => {
                    total_prompt = 0;
                    total_completion = 0;
                    eprintln!("  Conversation cleared. Next turn starts fresh.\n");
                }
                "/stats" => {
                    eprintln!("  Prompt tokens:     {total_prompt}");
                    eprintln!("  Completion tokens: {total_completion}");
                    eprintln!("  Temperature:       {temperature}");
                    eprintln!("  Top-p:             {top_p}");
                    eprintln!("  Max tokens:        {max_tokens}");
                    eprintln!(
                        "  Grammar:           {}",
                        grammar.as_deref().unwrap_or("off")
                    );
                    eprintln!();
                }
                "/temp" => {
                    if let Some(val) = parts.get(1) {
                        if let Ok(t) = val.parse::<f32>() {
                            temperature = t.clamp(0.0, 2.0);
                            eprintln!("  Temperature set to {temperature}\n");
                        } else {
                            eprintln!("  Invalid temperature: {val}\n");
                        }
                    } else {
                        eprintln!("  Usage: /temp <0.0-2.0>\n");
                    }
                }
                "/topp" => {
                    if let Some(val) = parts.get(1) {
                        if let Ok(p) = val.parse::<f32>() {
                            top_p = p.clamp(0.0, 1.0);
                            eprintln!("  Top-p set to {top_p}\n");
                        } else {
                            eprintln!("  Invalid top-p: {val}\n");
                        }
                    } else {
                        eprintln!("  Usage: /topp <0.0-1.0>\n");
                    }
                }
                "/max" => {
                    if let Some(val) = parts.get(1) {
                        if let Ok(n) = val.parse::<usize>() {
                            max_tokens = n;
                            eprintln!("  Max tokens set to {max_tokens}\n");
                        } else {
                            eprintln!("  Invalid max tokens: {val}\n");
                        }
                    } else {
                        eprintln!("  Usage: /max <n>\n");
                    }
                }
                "/grammar" => {
                    if parts.get(1) == Some(&"off") {
                        grammar = None;
                        eprintln!("  Grammar disabled.\n");
                    } else if let Some(path) = parts.get(1) {
                        match std::fs::read_to_string(path) {
                            Ok(g) => {
                                grammar = Some(g);
                                eprintln!("  Grammar loaded from {path}\n");
                            }
                            Err(e) => eprintln!("  Failed to read {path}: {e}\n"),
                        }
                    } else {
                        eprintln!("  Usage: /grammar <path> or /grammar off\n");
                    }
                }
                other => {
                    eprintln!("  Unknown command: {other}. Type /help for commands.\n");
                }
            }
            continue;
        }

        if input.is_empty() {
            continue;
        }

        // Build the completion request.
        // `session: Some("chat")` carries state across turns.
        // `preserve_state: true` tells the backend to chain state
        // between turns; `/clear` resets by starting a new session.
        let req = CompletionRequest {
            system: "You are a helpful, concise assistant.</think>".into(),
            prompt: input,
            output_schema: None,
            grammar: grammar.clone(),
            temperature,
            max_tokens,
            estimated_prompt_tokens: 0,
            thinking: false,
            preserve_state: true,
            on_token: Some(Box::new(move |token: &str| {
                print!("{token}");
                let _ = io::stdout().flush();
            })),
            session: Some(SESSION.to_string()),
        };

        // Streamed text is printed via on_token as tokens are produced.
        match backend.complete(req).await {
            Ok(resp) => {
                println!();
                total_prompt += resp.usage.prompt_tokens as u64;
                total_completion += resp.usage.completion_tokens as u64;
            }
            Err(e) => {
                eprintln!("\n  Error: {e}\n");
            }
        }
    }

    eprintln!("\nBye. (prompt={total_prompt}, completion={total_completion} tokens)\n");
    Ok(())
}
