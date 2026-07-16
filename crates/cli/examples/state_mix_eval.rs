//! State mixing: bake two personas separately, blend at tensor level, probe.
//!
//! Tests whether state-level blending produces persona fusion.

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

    // Load normalized roleplay data
    let data_path = Path::new("datasets/normalized/roleplay_normalized.jsonl");
    let conversations: Vec<Conversation> = load_jsonl(data_path)?;
    println!("Loaded {} conversations from {:?}\n", conversations.len(), data_path);

    // Pick two personas: Tony Stark (index 4) and Sherlock Holmes (index 0)
    let persona_a = &conversations[0]; // Sherlock
    let persona_b = &conversations[4]; // Tony Stark

    println!("=== Persona A: Sherlock Holmes ===");
    println!("System: {}\n", persona_a.system.chars().take(100).collect::<String>());

    println!("=== Persona B: Tony Stark ===");
    println!("System: {}\n", persona_b.system.chars().take(100).collect::<String>());

    // Bake each persona separately
    println!("Baking Sherlock Holmes...");
    let sherlock_examples: Vec<(&str, &str)> = persona_a
        .turns
        .iter()
        .map(|t| (t.user.as_str(), t.assistant.as_str()))
        .collect();
    bake_into_session(&backend, "sherlock", &persona_a.system, &sherlock_examples).await?;
    println!("Sherlock baked.\n");

    println!("Baking Tony Stark...");
    let tony_examples: Vec<(&str, &str)> = persona_b
        .turns
        .iter()
        .map(|t| (t.user.as_str(), t.assistant.as_str()))
        .collect();
    bake_into_session(&backend, "tony", &persona_b.system, &tony_examples).await?;
    println!("Tony baked.\n");

    // Probe each individually
    let probe = "Tell me about a challenge you overcame.";
    
    println!("=== Probe: Sherlock (unmixed) ===");
    let resp = backend.complete(CompletionRequest {
        system: String::new(),
        prompt: probe.to_string(),
        grammar: None,
        temperature: 0.7,
        max_tokens: 150,
        estimated_prompt_tokens: 0,
        thinking: false,
        preserve_state: true,
        output_schema: None,
        on_token: None,
        session: Some("sherlock".to_string()),
        bnf_mask: None,
    }).await?;
    println!("Response: {}\n", resp.text.trim());

    println!("=== Probe: Tony (unmixed) ===");
    let resp = backend.complete(CompletionRequest {
        system: String::new(),
        prompt: probe.to_string(),
        grammar: None,
        temperature: 0.7,
        max_tokens: 150,
        estimated_prompt_tokens: 0,
        thinking: false,
        preserve_state: true,
        output_schema: None,
        on_token: None,
        session: Some("tony".to_string()),
        bnf_mask: None,
    }).await?;
    println!("Response: {}\n", resp.text.trim());

    // Blend the two states
    println!("Blending states (alpha=0.5)...");
    backend.blend_states("sherlock", "tony", 0.5, "mixed")?;
    println!("Blended.\n");

    // Probe the blended state
    println!("=== Probe: Mixed (blended) ===");
    let resp = backend.complete(CompletionRequest {
        system: String::new(),
        prompt: probe.to_string(),
        grammar: None,
        temperature: 0.7,
        max_tokens: 150,
        estimated_prompt_tokens: 0,
        thinking: false,
        preserve_state: true,
        output_schema: None,
        on_token: None,
        session: Some("mixed".to_string()),
        bnf_mask: None,
    }).await?;
    println!("Response: {}\n", resp.text.trim());

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
