//! Interactive chat REPL for the local RWKV model.
//!
//! ```bash
//! cargo run -p roco-core --features grammar-rwkv --example chat --release
//! ```

use std::io::{self, BufRead, Write};

use roco_core::engine::{CompletionRequest, ModelBackend};
use roco_core::rwkv_backend::RwkvBackend;

/// How to structure the prompt for multi-turn conversations.
#[derive(Clone, Copy)]
enum PromptStyle {
    /// `System: ...\n\nUser: {current}\n\nAssistant:`
    /// Only current turn sent; relies on RNN state for history.
    StateOnly,
    /// `System: ...\n\nUser: t1\n\nAssistant: r1\n\nUser: t2\n\n...`
    /// Full interleaved history with system at top.
    Interleaved,
    /// `{history}\n\nSystem: ...\n\nUser: {current}\n\nAssistant:`
    /// Raw history dump, then system + current turn.
    HistoryFirst,
    /// `System: ...\n\nUser: t1\n\nAssistant: r1\n\nSystem: ...\n\nUser: t2\n\nAssistant: r2\n\n...`
    /// System prompt repeated before every user turn.
    RepeatedSystem,
}

impl PromptStyle {
    fn as_str(self) -> &'static str {
        match self {
            PromptStyle::StateOnly => "state-only",
            PromptStyle::Interleaved => "interleaved",
            PromptStyle::HistoryFirst => "history-first",
            PromptStyle::RepeatedSystem => "repeated-system",
        }
    }
    fn next(self) -> Self {
        match self {
            PromptStyle::StateOnly => PromptStyle::Interleaved,
            PromptStyle::Interleaved => PromptStyle::HistoryFirst,
            PromptStyle::HistoryFirst => PromptStyle::RepeatedSystem,
            PromptStyle::RepeatedSystem => PromptStyle::StateOnly,
        }
    }
}

fn print_banner() {
    eprintln!();
    eprintln!("  RoCo Chat -- local RWKV-7");
    eprintln!("  Type /help for commands. Ctrl+C to interrupt generation.");
    eprintln!();
}

fn print_help() {
    eprintln!();
    eprintln!("  /quit or /exit   Leave the chat");
    eprintln!("  /clear           Reset conversation");
    eprintln!("  /temp <n>        Temperature (0.0-2.0)");
    eprintln!("  /topp <n>        Top-p (0.0-1.0)");
    eprintln!("  /max <n>         Max tokens");
    eprintln!("  /grammar <path>  Load GBNF grammar file");
    eprintln!("  /grammar off     Disable grammar");
    eprintln!("  /style           Cycle prompt style (state-only / interleaved / history-first)");
    eprintln!("  /stats           Show token counts");
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

/// Turn record for building conversation history.
struct Turn {
    user: String,
    assistant: String,
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
    let mut style = PromptStyle::Interleaved;
    let mut turns: Vec<Turn> = Vec::new();

    print_banner();

    eprintln!("Loading model... (this takes a moment)");
    let backend = RwkvBackend::from_env()?;
    eprintln!("Backend: {} -- ready.  Style: {}\n", backend.name(), style.as_str());

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
                    turns.clear();
                    total_prompt = 0;
                    total_completion = 0;
                    eprintln!("  Conversation cleared.\n");
                }
                "/style" => {
                    style = style.next();
                    eprintln!("  Prompt style: {}\n", style.as_str());
                }
                "/stats" => {
                    eprintln!("  Prompt tokens:     {total_prompt}");
                    eprintln!("  Completion tokens: {total_completion}");
                    eprintln!("  Temperature:       {temperature}");
                    eprintln!("  Top-p:             {top_p}");
                    eprintln!("  Max tokens:        {max_tokens}");
                    eprintln!("  Prompt style:      {}", style.as_str());
                    eprintln!("  Turns in history:  {}", turns.len());
                    eprintln!();
                }
                "/temp" => {
                    if let Some(val) = parts.get(1) {
                        if let Ok(t) = val.parse::<f32>() {
                            temperature = t.clamp(0.0, 2.0);
                            eprintln!("  Temperature: {temperature}\n");
                        }
                    }
                }
                "/topp" => {
                    if let Some(val) = parts.get(1) {
                        if let Ok(p) = val.parse::<f32>() {
                            top_p = p.clamp(0.0, 1.0);
                            eprintln!("  Top-p: {top_p}\n");
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
                            Ok(g) => { grammar = Some(g); eprintln!("  Grammar loaded.\n"); }
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

        // Build the prompt text based on the current style.
        let system = "You are a helpful, concise assistant.</think>";
        let prompt = match style {
            PromptStyle::StateOnly => {
                input.clone()
            }
            PromptStyle::Interleaved => {
                let mut s = format!("System: {system}\n\n");
                for t in &turns {
                    s.push_str(&format!("User: {}\n\nAssistant: {}\n\n", t.user, t.assistant));
                }
                s.push_str(&format!("User: {input}"));
                s
            }
            PromptStyle::HistoryFirst => {
                let mut s = String::new();
                for t in &turns {
                    s.push_str(&format!("User: {}\n\nAssistant: {}\n\n", t.user, t.assistant));
                }
                s.push_str(&format!("System: {system}\n\nUser: {input}"));
                s
            }
            PromptStyle::RepeatedSystem => {
                let mut s = format!("System: {system}\n\n");
                for t in &turns {
                    s.push_str(&format!("User: {}\n\nAssistant: {}\n\nSystem: {system}\n\n", t.user, t.assistant));
                }
                s.push_str(&format!("User: {input}"));
                s
            }
        };

        let grammar_str = grammar.clone();

        let req = CompletionRequest {
            system: String::new(), // already baked into the prompt text
            prompt,
            output_schema: None,
            grammar: grammar_str,
            temperature,
            max_tokens,
            estimated_prompt_tokens: 0,
            thinking: false,
            preserve_state: false, // full context is in the text
            on_token: Some(Box::new(move |token: &str| {
                print!("{token}");
                let _ = io::stdout().flush();
            })),
            session: None,
        };

        match backend.complete(req).await {
            Ok(resp) => {
                println!();
                total_prompt += resp.usage.prompt_tokens as u64;
                total_completion += resp.usage.completion_tokens as u64;
                turns.push(Turn {
                    user: input,
                    assistant: resp.text,
                });
            }
            Err(e) => {
                eprintln!("\n  Error: {e}\n");
            }
        }
    }

    eprintln!("\nBye. (prompt={total_prompt}, completion={total_completion}, turns={})\n", turns.len());
    Ok(())
}
