//! Quick inference test: load the model (SafeTensors), run a short completion.

use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;

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
        on_token: None,
        session: None,
        ..Default::default()
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
