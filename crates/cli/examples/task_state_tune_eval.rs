//! Evaluate task-specific state-tunes by baking datasets and probing
//!
//! Usage: cargo run --example task_state_tune_eval --release

use std::fs;
use std::path::Path;

use roco_engine::{bake_into_session, CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ConversationTurn {
    user: String,
    assistant: String,
}

#[derive(Debug, Deserialize)]
struct DatasetEntry {
    system: String,
    turns: Vec<ConversationTurn>,
}

/// Pre-think block to pre-fill during probing. This makes the model believe
/// it has already done its thinking and continue directly into content.
/// Note: The opening  must be properly escaped in the string literal.
const PRETHINK_BLOCK: &str = "\n<think>Generating content.</think>\n";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Loading RWKV backend...");
    let backend = RwkvBackend::from_env()?;

    let datasets_dir = Path::new("datasets/tasks");
    let dataset_files = fs::read_dir(datasets_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "jsonl"))
        .collect::<Vec<_>>();

    println!("Found {} dataset files\n", dataset_files.len());

    for dataset_file in dataset_files {
        let dataset_name = dataset_file
            .file_name()
            .to_string_lossy()
            .replace(".jsonl", "");
        println!("=== {} ===", dataset_name);

        // Load dataset
        let content = fs::read_to_string(dataset_file.path())?;
        let entries: Vec<DatasetEntry> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l))
            .collect::<Result<_, _>>()?;

        if entries.is_empty() {
            println!("  No entries found\n");
            continue;
        }

        // Extract system prompt and turns
        let system = &entries[0].system;
        let mut examples = Vec::new();
        for entry in &entries {
            for turn in &entry.turns {
                examples.push((turn.user.as_str(), turn.assistant.as_str()));
            }
        }

        println!("  Baking {} examples...", examples.len());
        let session_name = format!("task_{}", dataset_name);
        bake_into_session(&backend, &session_name, system, &examples).await?;
        println!("  Baked into session '{}'\n", session_name);

        // Probe with test input, using pre-think block
        let probe = match dataset_name.as_str() {
            "story_writing" => "Write a chapter where Mara discovers the shadow box is alive.",
            "plot_overview" => "Create an outline for a story about a time-traveling librarian.",
            "wiki_generation" => "Generate a wiki entry for the Shadow Guild.",
            "project_planning" => "Plan the implementation of a multi-agent story generation system.",
            "summarization" => "Summarize: Mara stole the shadow box from the Grand Archive. The shadows began spreading through Duskfall City. The Umbral Order imposed martial law. Mara had to choose between returning the box or letting the shadows reveal the city's buried history.",
            "dataset_generation" => "Generate 3 new training examples for the story_writing task. The model should learn to write story chapters from a plan and wiki.",
            _ => "Test input.",
        };

        println!("  Probe: {}", probe);
        let response = backend
            .complete(CompletionRequest {
                prompt: probe.to_string(),
                prefill: Some(PRETHINK_BLOCK.to_string()),
                max_tokens: 150,
                temperature: 0.7,
                session: Some(session_name.clone()),
                preserve_state: true,
                ..Default::default()
            })
            .await?;

        println!("  Response: {}\n", response.text.trim());
    }

    Ok(())
}
