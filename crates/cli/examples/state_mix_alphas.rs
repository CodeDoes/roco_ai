//! Test state mixing at different alpha values.
//! Does the blend track toward one persona, or does it always collapse?

use roco_engine::{bake_into_session, CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;
use serde::Deserialize;
use std::path::Path;

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

    let data_path = Path::new("datasets/normalized/roleplay_normalized.jsonl");
    let conversations: Vec<Conversation> = load_jsonl(data_path)?;
    println!("Loaded {} conversations\n", conversations.len());

    // Sherlock (0) and Tony (4)
    let sherlock = &conversations[0];
    let tony = &conversations[4];

    println!("Baking Sherlock...");
    let sherlock_examples: Vec<(&str, &str)> = sherlock
        .turns
        .iter()
        .map(|t| (t.user.as_str(), t.assistant.as_str()))
        .collect();
    bake_into_session(&backend, "sherlock", &sherlock.system, &sherlock_examples).await?;
    println!("Baking Tony...");
    let tony_examples: Vec<(&str, &str)> = tony
        .turns
        .iter()
        .map(|t| (t.user.as_str(), t.assistant.as_str()))
        .collect();
    bake_into_session(&backend, "tony", &tony.system, &tony_examples).await?;
    println!("Both baked.\n");

    let probe = "Tell me about a challenge you overcame.";
    let alphas = [0.1, 0.25, 0.5, 0.75, 0.9];

    for alpha in alphas {
        let session = format!("mix_{:.2}", alpha);
        println!("=== alpha = {} ===", alpha);
        backend.blend_states("sherlock", "tony", alpha, &session)?;
        
        let resp = backend.complete(CompletionRequest {
            system: String::new(),
            prompt: probe.to_string(),
            prefill: None,
            grammar: None,
            temperature: 0.7,
            max_tokens: 150,
            estimated_prompt_tokens: 0,
            thinking: false,
            preserve_state: true,
            output_schema: None,
            on_token: None,
            session: Some(session),
            bnf_mask: None,
        }).await?;
        println!("Response: {}\n", resp.text.trim());
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
