//! Interactive chat REPL for the local RWKV model.
//!
//! Multi-turn conversations persist via the Phase-1 session state pool in
//! `crates/session` (the backend saves/restores recurrent state keyed by the
//! `session` id), so the model's own state — not a rebuilt prompt — carries
//! the conversation. Supports `/save`, `/load`, `/system`, grammar
//! constraints, streaming, temperature/max-token tuning, and stats.

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;

fn print_help() {
    eprintln!("\n  /quit or /exit   Leave");
    eprintln!("  /clear           Start a fresh conversation");
    eprintln!("  /save <name>     Name the current conversation (switch to it)");
    eprintln!("  /load <name>     Switch to a named conversation (creates if new)");
    eprintln!("  /system <text>   Set the system / instruction prompt");
    eprintln!("  /temp <n>        Temperature (0.0-2.0)");
    eprintln!("  /max <n>         Max tokens per response");
    eprintln!("  /grammar <file>  Load a GBNF grammar (or 'off')");
    eprintln!("  /stats           Show token counts");
    eprintln!("  /help            Show this\n");
}

fn do_prompt() -> io::Result<String> {
    print!("> ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim_end().to_string())
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
    let mut grammar: Option<String> = None;
    let mut total_prompt = 0u64;
    let mut total_completion = 0u64;

    // Session persistence (Phase-1 state pool in the backend). `baked` tracks
    // whether the system prompt has already been folded into the recurrent
    // state for the active session, so it is only sent on the first turn.
    let mut current_session = "chat".to_string();
    let mut system = "You are a helpful, concise assistant.".to_string();
    let mut baked = false;

    eprintln!("\n  RoCo Chat -- local RWKV-7\n  Type /help for commands. Ctrl+C to interrupt.\n");
    eprintln!("Loading model... (this takes a moment)");
    let backend = RwkvBackend::from_env()?;
    eprintln!(
        "Backend: {} -- ready.  Session: {}\n",
        backend.name(),
        current_session
    );

    loop {
        let input = match do_prompt() {
            Ok(line) => line,
            Err(e) => {
                eprintln!("Input error: {e}");
                break;
            }
        };

        if input.starts_with('/') {
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            match parts[0] {
                "/quit" | "/exit" => break,
                "/help" => print_help(),
                "/clear" => {
                    let stamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0);
                    current_session = format!("chat-{stamp:x}");
                    baked = false;
                    eprintln!("  Conversation cleared (new session {current_session}).\n");
                }
                "/save" => match parts.get(1) {
                    Some(name) => {
                        current_session = name.to_string();
                        eprintln!("  Named current conversation '{name}'.\n");
                    }
                    None => eprintln!("  Usage: /save <name>\n"),
                },
                "/load" => match parts.get(1) {
                    Some(name) => {
                        current_session = name.to_string();
                        baked = false;
                        eprintln!("  Loaded conversation '{name}'.\n");
                    }
                    None => eprintln!("  Usage: /load <name>\n"),
                },
                "/system" => match parts.get(1) {
                    Some(text) => {
                        system = text.to_string();
                        baked = false;
                        eprintln!("  System prompt set.\n");
                    }
                    None => eprintln!("  Usage: /system <text>\n"),
                },
                "/stats" => eprintln!(
                    "  Prompt tokens: {total_prompt}\n  Completion tokens: {total_completion}\n  Temperature: {temperature}\n  Max tokens: {max_tokens}\n  Session: {current_session}\n  Grammar: {}\n",
                    grammar.is_some()
                ),
                "/temp" => {
                    if let Some(val) = parts.get(1) {
                        if let Ok(t) = val.parse::<f32>() {
                            temperature = t.clamp(0.0, 2.0);
                            eprintln!("  Temperature: {temperature}\n");
                        }
                    }
                }
                "/max" => {
                    if let Some(val) = parts.get(1) {
                        if let Ok(n) = val.parse::<usize>() {
                            max_tokens = n;
                            eprintln!("  Max tokens: {max_tokens}\n");
                        }
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
                                eprintln!("  Grammar loaded.\n");
                            }
                            Err(e) => eprintln!("  Failed: {e}\n"),
                        }
                    }
                }
                other => eprintln!("  Unknown: {other}. Type /help.\n"),
            }
            continue;
        }

        if input.is_empty() {
            continue;
        }

        // On the first turn of a session, fold the system prompt into the
        // recurrent state; afterwards the state itself carries it.
        let (req_system, req_prompt) = if !baked {
            (system.clone(), input.clone())
        } else {
            (String::new(), input.clone())
        };

        let captured = Arc::new(Mutex::new(String::new()));
        let cloned = captured.clone();

        let resp = backend
            .complete(CompletionRequest {
                system: req_system,
                prompt: req_prompt,
                output_schema: None,
                grammar: grammar.clone(),
                temperature,
                max_tokens,
                estimated_prompt_tokens: 0,
                thinking: false,
                preserve_state: false,
                on_token: Some(Box::new(move |token: &str| {
                    print!("{token}");
                    let _ = io::stdout().flush();
                    cloned.lock().unwrap().push_str(token);
                })),
                session: Some(current_session.clone()),
            })
            .await;

        match resp {
            Ok(r) => {
                println!();
                total_prompt += r.usage.prompt_tokens as u64;
                total_completion += r.usage.completion_tokens as u64;
                baked = true;
                let _ = captured;
            }
            Err(e) => eprintln!("\n  Error: {e}\n"),
        }
    }

    eprintln!(
        "\nBye. (prompt={total_prompt}, completion={total_completion}, session={current_session})\n"
    );
    Ok(())
}
