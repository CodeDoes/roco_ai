//! State-tuning eval: bake with different subset sizes, probe with held-out prompts.
//!
//! Usage: `cargo run --example state_tune_eval --release`

use roco_engine::{bake_into_session, CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;
use serde::Deserialize;
use std::path::Path;

#[allow(dead_code)]
#[derive(Deserialize)]
struct Conversation {
    system: String,
    turns: Vec<Turn>,
}

#[derive(Deserialize)]
struct Turn {
    user: String,
    assistant: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Loading RWKV backend...");
    let backend = RwkvBackend::from_env()?;
    println!("Backend loaded.\n");

    // Load normalized roleplay data
    let data_path = Path::new("datasets/normalized/roleplay_normalized.jsonl");
    let conversations: Vec<Conversation> = load_jsonl(data_path)?;
    println!(
        "Loaded {} conversations from {:?}\n",
        conversations.len(),
        data_path
    );

    // Test subset sizes
    let subset_sizes = [5, 10, 20];
    let probe_prompts = [
        "What's your favorite way to spend a rainy afternoon?",
        "Tell me about a challenge you overcame.",
        "If you could have dinner with anyone, who would it be?",
    ];

    for &n in &subset_sizes {
        if n > conversations.len() {
            println!(
                "Skipping subset size {} (only {} conversations available)\n",
                n,
                conversations.len()
            );
            continue;
        }

        println!("=== Subset size: {} conversations ===", n);
        let subset = &conversations[..n];

        // Convert to (user, assistant) pairs for bake_into_session
        // NO system prompt — the state itself is the persona
        let mut examples = Vec::new();
        for conv in subset {
            for turn in &conv.turns {
                examples.push((turn.user.as_str(), turn.assistant.as_str()));
            }
        }

        println!(
            "Baking {} (user, assistant) pairs into session 'tune_{}'...",
            examples.len(),
            n
        );
        let session = format!("tune_{}", n);
        bake_into_session(&backend, &session, "", &examples).await?;
        println!("Baked.\n");

        // Probe with held-out prompts
        for (i, prompt) in probe_prompts.iter().enumerate() {
            println!("Probe {}: {}", i + 1, prompt);
            let req = CompletionRequest {
                prompt: prompt.to_string(),
                temperature: 0.7,
                max_tokens: 150,
                preserve_state: true,
                session: Some(session.clone()),
                ..Default::default()
            };
            let resp = backend.complete(req).await?;
            println!("Response: {}\n", resp.text.trim());
        }

        println!("{}\n", "=".repeat(60));
    }

    Ok(())
}

fn load_jsonl<P: AsRef<Path>>(path: P) -> anyhow::Result<Vec<Conversation>> {
    let content = std::fs::read_to_string(path)?;
    let mut result = Vec::new();
    for line in content.lines() {
        if !line.trim().is_empty() {
            result.push(serde_json::from_str(line)?);
        }
    }
    Ok(result)
}
