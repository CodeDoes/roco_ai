//! Quick inference test: load the converted model, run a short completion.
//!
//! ```bash
//! RWKV_QUANT=all cargo run -p roco-core --features local-rwkv --example rwkv_test
//! ```

use roco_core::engine::{CompletionRequest, ModelBackend};
use roco_core::rwkv_backend::RwkvBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Building RwkvBackend…");
    let backend = RwkvBackend::from_env()?;
    println!("backend: {}", backend.name());

    let req = CompletionRequest {
        system: "You are a helpful assistant.".into(),
        prompt: "The capital of France is".into(),
        output_schema: None,
        grammar: None,
        temperature: 0.5,
        max_tokens: 32,
        estimated_prompt_tokens: 8,
        thinking: false,
            preserve_state: false,
    };

    println!("prompting…");
    let resp = backend.complete(req).await?;
    println!("\n--- response ---\n{}", resp.text);
    println!(
        "\n--- usage: {} prompt, {} completion ---",
        resp.usage.prompt_tokens, resp.usage.completion_tokens
    );

    Ok(())
}
