//! Interactive chat REPL for the local RWKV model.

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::Mutex;

use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;

#[derive(Clone, Copy)]
enum PromptStyle { StateOnly, Interleaved, HistoryFirst, RepeatedSystem }

impl PromptStyle {
    fn as_str(self) -> &'static str {
        match self { PromptStyle::StateOnly => "state-only", PromptStyle::Interleaved => "interleaved", PromptStyle::HistoryFirst => "history-first", PromptStyle::RepeatedSystem => "repeated-system" }
    }
    fn next(self) -> Self {
        match self { PromptStyle::StateOnly => PromptStyle::Interleaved, PromptStyle::Interleaved => PromptStyle::HistoryFirst, PromptStyle::HistoryFirst => PromptStyle::RepeatedSystem, PromptStyle::RepeatedSystem => PromptStyle::StateOnly }
    }
}

struct Turn { user: String, assistant: String }

fn print_help() {
    eprintln!("\n  /quit or /exit   Leave");
    eprintln!("  /clear           Reset conversation");
    eprintln!("  /temp <n>        Temperature (0.0-2.0)");
    eprintln!("  /max <n>         Max tokens");
    eprintln!("  /style           Cycle prompt style");
    eprintln!("  /stats           Show token counts");
    eprintln!("  /help            Show this\n");
}

fn do_prompt() -> io::Result<String> { print!("> "); io::stdout().flush()?; let mut line = String::new(); io::stdin().lock().read_line(&mut line)?; Ok(line.trim_end().to_string()) }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")))
        .init();

    let mut temperature = 0.5f32;
    let mut max_tokens = 512usize;
    let mut grammar: Option<String> = None;
    let mut total_prompt = 0u64;
    let mut total_completion = 0u64;
    let mut style = PromptStyle::Interleaved;
    let mut turns: Vec<Turn> = Vec::new();

    eprintln!("\n  RoCo Chat -- local RWKV-7\n  Type /help for commands. Ctrl+C to interrupt.\n");
    eprintln!("Loading model... (this takes a moment)");
    let backend = RwkvBackend::from_env()?;
    eprintln!("Backend: {} -- ready.  Style: {}\n", backend.name(), style.as_str());

    loop {
        let input = match do_prompt() { Ok(line) => line, Err(e) => { eprintln!("Input error: {e}"); break } };

        if input.starts_with('/') {
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            match parts[0] {
                "/quit" | "/exit" => break,
                "/help" => print_help(),
                "/clear" => { turns.clear(); total_prompt = 0; total_completion = 0; eprintln!("  Conversation cleared.\n"); }
                "/style" => { style = style.next(); eprintln!("  Prompt style: {}\n", style.as_str()); }
                "/stats" => { eprintln!("  Prompt tokens: {total_prompt}\n  Completion tokens: {total_completion}\n  Temperature: {temperature}\n  Max tokens: {max_tokens}\n  Style: {}\n  Turns: {}\n", style.as_str(), turns.len()); }
                "/temp" => { if let Some(val) = parts.get(1) { if let Ok(t) = val.parse::<f32>() { temperature = t.clamp(0.0, 2.0); eprintln!("  Temperature: {temperature}\n"); } } }
                "/max" => { if let Some(val) = parts.get(1) { if let Ok(n) = val.parse::<usize>() { max_tokens = n; eprintln!("  Max tokens: {max_tokens}\n"); } } }
                "/grammar" => if parts.get(1) == Some(&"off") {
                    grammar = None; eprintln!("  Grammar disabled.\n");
                } else if let Some(path) = parts.get(1) {
                    match std::fs::read_to_string(path) {
                        Ok(g) => { grammar = Some(g); eprintln!("  Grammar loaded.\n"); }
                        Err(e) => eprintln!("  Failed: {e}\n"),
                    }
                }
                other => eprintln!("  Unknown: {other}. Type /help.\n"),
            }
            continue;
        }

        if input.is_empty() { continue; }

        let system = "You are a helpful, concise assistant.</think>";
        let prompt = match style {
            PromptStyle::StateOnly => input.clone(),
            PromptStyle::Interleaved => {
                let mut s = format!("System: {system}\n\n");
                for t in &turns { s.push_str(&format!("User: {}\n\nAssistant: {}\n\n", t.user, t.assistant)); }
                s.push_str(&format!("User: {input}")); s
            }
            PromptStyle::HistoryFirst => {
                let mut s = String::new();
                for t in &turns { s.push_str(&format!("User: {}\n\nAssistant: {}\n\n", t.user, t.assistant)); }
                s.push_str(&format!("System: {system}\n\nUser: {input}")); s
            }
            PromptStyle::RepeatedSystem => {
                let mut s = format!("System: {system}\n\n");
                for t in &turns { s.push_str(&format!("User: {}\n\nAssistant: {}\n\nSystem: {system}\n\n", t.user, t.assistant)); }
                s.push_str(&format!("User: {input}")); s
            }
        };

        let streamed = Arc::new(Mutex::new(String::new()));
        let cloned = streamed.clone();

        let resp = backend.complete(CompletionRequest {
            system: String::new(), prompt, output_schema: None,
            grammar: grammar.clone(), temperature, max_tokens,
            estimated_prompt_tokens: 0, thinking: false, preserve_state: false,
            on_token: Some(Box::new(move |token: &str| {
                print!("{token}"); let _ = io::stdout().flush();
                cloned.lock().unwrap().push_str(token);
            })),
            session: None,
        }).await;

        match resp {
            Ok(r) => {
                println!();
                total_prompt += r.usage.prompt_tokens as u64;
                total_completion += r.usage.completion_tokens as u64;
                let text = streamed.lock().unwrap().clone();
                turns.push(Turn { user: input, assistant: text });
            }
            Err(e) => eprintln!("\n  Error: {e}\n"),
        }
    }

    eprintln!("\nBye. (prompt={total_prompt}, completion={total_completion}, turns={})\n", turns.len());
    Ok(())
}
