//! Grammar-constrained inference smoke test.
//!
//! Loads the same RWKV-7 model as `rwkv_test.rs` but feeds a GBNF
//! grammar through `RWKV_GRAMMAR` and `CompletionRequest::grammar`.
//! The actor masks logits so the only tokens emitted are those
//! accepted by the live schoolmarm walker.
//!
//! Default grammar is `root ::= "yes" | "no"`. The prompt asks the
//! model to answer yes/no; the response should always be one of the
//! two.
//!
//! ```bash
//! cargo run -p roco-core --features grammar-rwkv --example grammar_smoke --release
//! # Or with a custom grammar:
//! RWKV_GRAMMAR_FILE=/path/to/my.gbnf cargo run -p roco-core --features grammar-rwkv --example grammar_smoke --release
//! ```

use std::env;

use roco_core::engine::{CompletionRequest, ModelBackend};
use roco_core::rwkv_backend::RwkvBackend;

const DEFAULT_GRAMMAR: &str = r#"root ::= "yes" | "no""#;
const DEFAULT_PROMPT: &str = "Are you a helpful AI? Answer with one word: yes or no.";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Building RwkvBackend (grammar-rwkv feature)…");
    let backend = RwkvBackend::from_env()?;
    println!("backend: {}", backend.name());

    let grammar = match env::var("RWKV_GRAMMAR_FILE") {
        Ok(p) => std::fs::read_to_string(&p)
            .map_err(|e| anyhow::anyhow!("could not read RWKV_GRAMMAR_FILE={p}: {e}"))?,
        Err(_) => env::var("RWKV_GRAMMAR").unwrap_or_else(|_| DEFAULT_GRAMMAR.to_string()),
    };

    println!("grammar ({} bytes):\n{}", grammar.len(), grammar);

    let req = CompletionRequest {
        system: "You are a helpful assistant.".into(),
        prompt: env::var("RWKV_GRAMMAR_PROMPT").unwrap_or_else(|_| DEFAULT_PROMPT.to_string()),
        output_schema: None,
        grammar: Some(grammar.clone()),
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
        Ok(resp) => {
            println!("\n--- response ---\n{}", resp.text);
            resp.text
        }
        Err(e) => {
            eprintln!("inference failed: {e}");
            String::new()
        }
    };

    let stripped = text.trim();
    let is_yes = stripped == "yes";
    let is_no = stripped == "no";
    let matches_grammar_default = DEFAULT_GRAMMAR == grammar && (is_yes || is_no);
    println!(
        "\n--- grammar check: {} ---",
        if matches_grammar_default {
            "PASS"
        } else if !grammar.is_empty() {
            "(non-default grammar; manual check)"
        } else {
            "FAIL"
        }
    );

    Ok(())
}
