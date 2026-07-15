//! Story generation using the mechanistic agent pattern.
//!
//! Runs the RWKV backend directly with a structured workflow:
//! think (outline) → write (chapter prose) → commit (files).
//! No JSON plan grammar — the model writes natural language; the code
//! structures the output.

use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    let prompt = std::env::args().nth(1).unwrap_or_else(|| {
        "Write a short story about a lighthouse keeper who discovers a message in a bottle.".to_string()
    });

    println!("Loading model... (this takes a moment)");
    let backend = RwkvBackend::from_env()?;
    println!("Model: {} ready.\n", backend.name());

    // Phase 1: Think — generate a story outline.
    println!("=== PHASE 1: THINK (outline) ===\n");
    let think_prompt = format!(
        "Outline a short story with 3 chapters based on this premise:\n{}\n\n\
         Give me: title, genre, tone, and a one-line summary of each chapter.",
        prompt
    );
    let outline = backend.complete(CompletionRequest {
        system: "You are a creative writing assistant. Output a clear, structured outline.".into(),
        prompt: think_prompt,
        temperature: 0.7,
        max_tokens: 400,
        ..Default::default()
    }).await?;
    println!("{}\n", outline.text);

    // Phase 2: Write chapter 1.
    println!("=== PHASE 2: CHAPTER 1 ===\n");
    let ch1 = backend.complete(CompletionRequest {
        system: "You are a fiction writer. Write vivid, engaging prose.".into(),
        prompt: format!(
            "Based on this outline:\n{}\n\nWrite Chapter 1. Introduce the main character and setting. ~300 words.",
            outline.text
        ),
        temperature: 0.8,
        max_tokens: 600,
        ..Default::default()
    }).await?;
    println!("{}\n", ch1.text);

    // Phase 3: Write chapter 2 (continuing from state).
    println!("=== PHASE 3: CHAPTER 2 ===\n");
    let ch2 = backend.complete(CompletionRequest {
        system: String::new(),
        prompt: format!(
            "Continue the story. Write Chapter 2. Advance the plot from where Chapter 1 left off. ~300 words.\n\nChapter 1 recap: {}",
            ch1.text.chars().take(200).collect::<String>()
        ),
        temperature: 0.8,
        max_tokens: 600,
        preserve_state: true,
        ..Default::default()
    }).await?;
    println!("{}\n", ch2.text);

    // Phase 4: Write chapter 3 (conclusion).
    println!("=== PHASE 4: CHAPTER 3 ===\n");
    let ch3 = backend.complete(CompletionRequest {
        system: String::new(),
        prompt: "Write Chapter 3 — the conclusion. Resolve the story with a satisfying ending. ~300 words.".into(),
        temperature: 0.8,
        max_tokens: 600,
        preserve_state: true,
        ..Default::default()
    }).await?;
    println!("{}\n", ch3.text);

    // Phase 5: Generate a synopsis.
    println!("=== PHASE 5: SYNOPSIS ===\n");
    let synopsis = backend.complete(CompletionRequest {
        system: String::new(),
        prompt: "Now write a one-paragraph synopsis of the complete story you just wrote.".into(),
        temperature: 0.5,
        max_tokens: 200,
        preserve_state: true,
        ..Default::default()
    }).await?;
    println!("{}\n", synopsis.text);

    println!("=== DONE ===\nStory complete in {} chapters.", 3);
    Ok(())
}
