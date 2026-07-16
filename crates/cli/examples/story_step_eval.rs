//! Individual story pipeline step evaluator
//! Tests each step in isolation to identify bugs

use roco_inference::RwkvBackend;
use roco_engine::{CompletionRequest, ModelBackend};

// Copy grammars from story.rs
const OUTLINE_GRAMMAR: &str = r#"
root  ::= "{" space "\"title\"" space ":" space string space "," space "\"genre\"" space ":" space string space "," space "\"tone\"" space ":" space string space "," space "\"chapters\"" space ":" space "[" space chapter ( "," space chapter )* "]" space "}"
chapter ::= "{" space "\"number\"" space ":" space number space "," space "\"title\"" space ":" space string space "," space "\"summary\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
number ::= [0-9]+
space ::= " "?
"#;

const WIKI_GRAMMAR: &str = r#"
root  ::= "{" space "\"characters\"" space ":" space "[" space character ( "," space character )* "]" space "," space "\"setting\"" space ":" space string space "}"
character ::= "{" space "\"name\"" space ":" space string space "," space "\"description\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

const CHAPTER_GRAMMAR: &str = r#"
root  ::= "{" space "\"title\"" space ":" space string space "," space "\"content\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

const VALIDATION_GRAMMAR: &str = r#"
root  ::= "{" space "\"quality\"" space ":" space quality space "," space "\"issues\"" space ":" space string space "," space "\"suggestion\"" space ":" space string space "}"
quality ::= "\"pass\"" | "\"fail\"" | "\"needs-work\""
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

const SYNOPSIS_GRAMMAR: &str = r#"
root  ::= "{" space "\"summary\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let step = std::env::args().nth(1).unwrap_or_else(|| "all".to_string());
    let prompt = std::env::args().nth(2).unwrap_or_else(|| "A cat who learns to fly".to_string());
    
    println!("Loading model...");
    let backend = RwkvBackend::from_env()?;
    println!("Model ready.\n");

    match step.as_str() {
        "outline" => test_step(&backend, "OUTLINE", OUTLINE_GRAMMAR, &prompt, 0.7, 500).await,
        "wiki" => test_step(&backend, "WIKI", WIKI_GRAMMAR, &prompt, 0.7, 500).await,
        "chapter" => test_step(&backend, "CHAPTER", CHAPTER_GRAMMAR, &prompt, 0.8, 1000).await,
        "validation" => test_step(&backend, "VALIDATION", VALIDATION_GRAMMAR, &prompt, 0.5, 300).await,
        "synopsis" => test_step(&backend, "SYNOPSIS", SYNOPSIS_GRAMMAR, &prompt, 0.6, 400).await,
        "all" => {
            test_step(&backend, "OUTLINE", OUTLINE_GRAMMAR, &prompt, 0.7, 500).await;
            test_step(&backend, "WIKI", WIKI_GRAMMAR, &prompt, 0.7, 500).await;
            test_step(&backend, "CHAPTER", CHAPTER_GRAMMAR, &prompt, 0.8, 1000).await;
            test_step(&backend, "VALIDATION", VALIDATION_GRAMMAR, &prompt, 0.5, 300).await;
            test_step(&backend, "SYNOPSIS", SYNOPSIS_GRAMMAR, &prompt, 0.6, 400).await;
        }
        _ => {
            eprintln!("Unknown step: {}. Use: outline, wiki, chapter, validation, synopsis, all", step);
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn test_step(
    backend: &RwkvBackend,
    name: &str,
    grammar: &str,
    prompt: &str,
    temperature: f32,
    max_tokens: usize,
) {
    println!("═══════════════════════════════════════════════════════════");
    println!("Testing: {}", name);
    println!("═══════════════════════════════════════════════════════════");
    println!("Prompt: {}\n", prompt);

    let result = backend.complete(CompletionRequest {
        system: format!("You are a story generator. Output only valid JSON matching the grammar. Prompt: {}", prompt),
        prompt: format!("Generate {} JSON:", name.to_lowercase()),
        grammar: Some(grammar.to_string()),
        temperature,
        max_tokens,
        ..Default::default()
    }).await;

    match result {
        Ok(resp) => {
            println!("✓ Completion succeeded");
            println!("Tokens: {} prompt, {} completion", resp.usage.prompt_tokens, resp.usage.completion_tokens);
            println!("\nRaw output (first 500 chars):");
            let preview = resp.text.chars().take(500).collect::<String>();
            println!("{}\n", preview);
            
            // Try to parse as JSON
            match serde_json::from_str::<serde_json::Value>(&resp.text) {
                Ok(json) => {
                    println!("✓ Valid JSON");
                    println!("Parsed structure:");
                    println!("{}", serde_json::to_string_pretty(&json).unwrap());
                }
                Err(e) => {
                    println!("✗ Invalid JSON: {}", e);
                    println!("\nFull raw text:");
                    println!("{}", resp.text);
                }
            }
        }
        Err(e) => {
            println!("✗ Completion failed: {}", e);
        }
    }
    println!();
}
