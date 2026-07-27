//! Single-step generative debug REPL: `roco debug`.

use crate::daemon;
use roco_engine::{CompletionRequest, ModelBackend};

/// Run the generative debug REPL using the provided backend.
pub fn run_debug_with_backend(backend: &dyn ModelBackend, prompt: &str) {
    println!("================================================================");
    println!("  RoCo AI — Generative Debug REPL");
    println!("================================================================");
    println!("  Commands: <Enter> (step) | continue | state | grammar | quit");
    println!("================================================================");

    let req = CompletionRequest::builder()
        .system("You are a debug assistant.")
        .prompt(prompt)
        .temperature(0.7)
        .max_tokens(10)
        .record_trace(true)
        .build();

    let resp = match futures::executor::block_on(backend.complete(req)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error starting debug session: {e}");
            return;
        }
    };

    println!("  Prompt: {:?}", prompt);
    println!("  Backend: {}", backend.name());
    println!("----------------------------------------------------------------");

    if resp.trace.is_empty() {
        println!("  Generated: {:?}", resp.text);
    } else {
        for (i, t) in resp.trace.iter().enumerate() {
            println!(
                "  step {:02} | token={:<5} {:<12} | p={:.4} | grammar_masked={}",
                i + 1,
                t.token_id,
                format!("{:?}", t.token_str),
                t.probability,
                t.grammar_masked
            );
        }
    }
    println!("================================================================");
}

/// Run the generative debug REPL.
pub fn cmd_debug(extra: &[&str]) {
    let prompt = extra
        .iter()
        .find(|&&a| !a.starts_with('-'))
        .copied()
        .unwrap_or("Write a sentence about debug mode.");

    let backend = daemon::ensure_sync_backend();
    run_debug_with_backend(&*backend, prompt);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmd_debug_runs_without_panic() {
        let mock = roco_engine::MockBackend::default();
        run_debug_with_backend(&mock, "Test prompt");
    }
}
