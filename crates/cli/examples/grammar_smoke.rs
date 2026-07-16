//! Grammar-constrained inference smoke test.

use std::env;
use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;

const DEFAULT_GRAMMAR: &str = r#"root ::= "yes" | "no""#;
const DEFAULT_PROMPT: &str = "Are you a helpful AI? Answer with one word: yes or no.";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Building RwkvBackend…");
    let backend = RwkvBackend::from_env()?;
    println!("backend: {}", backend.name());

    let grammar = match env::var("RWKV_GRAMMAR_FILE") {
        Ok(p) => std::fs::read_to_string(&p).map_err(|e| anyhow::anyhow!("could not read RWKV_GRAMMAR_FILE={p}: {e}"))?,
        Err(_) => env::var("RWKV_GRAMMAR").unwrap_or_else(|_| DEFAULT_GRAMMAR.to_string()),
    };

    println!("grammar ({} bytes):\n{}", grammar.len(), grammar);

    let req = CompletionRequest {
        system: "You are a helpful assistant.".into(),
        prompt: env::var("RWKV_GRAMMAR_PROMPT").unwrap_or_else(|_| DEFAULT_PROMPT.to_string()),
        output_schema: None,
        grammar: Some(grammar.clone()),
        bnf_mask: None,
        temperature: 1.0,
        max_tokens: 8,
        estimated_prompt_tokens: 32,
        thinking: false,
        preserve_state: false,
        on_token: None,
        session: None,
    };

    println!("prompting…");
    let text = match backend.complete(req).await {
        Ok(resp) => { println!("\n--- response ---\n{}", resp.text); resp.text }
        Err(e) => { eprintln!("inference failed: {e}"); String::new() }
    };

    let stripped = text.trim();
    let passes = grammar == DEFAULT_GRAMMAR && (stripped == "yes" || stripped == "no");
    println!("\n--- grammar check: {} ---", if passes { "PASS" } else if !grammar.is_empty() { "(non-default grammar)" } else { "FAIL" });
    Ok(())
}
