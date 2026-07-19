//! Story pipeline step-by-step evaluator
//! Tests each step individually to isolate bugs

use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;

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

#[tokio::main]
async fn main() {
    println!("=== Story Pipeline Step-by-Step Eval ===\n");

    let backend = match RwkvBackend::from_env() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("✗ Failed to init backend: {}", e);
            return;
        }
    };
    println!("✓ Backend initialized\n");

    let prompt = "A cat who learns to fly";

    // Step 1: Outline
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("STEP 1: OUTLINE");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eval_step(
        &backend,
        "Outline",
        OUTLINE_GRAMMAR,
        &format!(
            "Generate a story outline for: {}\nRespond with JSON only.",
            prompt
        ),
        0.7,
        500,
    )
    .await;

    // Step 2: Wiki
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("STEP 2: WIKI");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eval_step(
        &backend,
        "Wiki",
        WIKI_GRAMMAR,
        &format!(
            "Create characters and setting for a story about: {}\nRespond with JSON only.",
            prompt
        ),
        0.7,
        500,
    )
    .await;

    // Step 3: Chapter 1
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("STEP 3: CHAPTER 1");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eval_step(
        &backend,
        "Chapter",
        CHAPTER_GRAMMAR,
        &format!(
            "Write chapter 1 of a story about: {}\nRespond with JSON only.",
            prompt
        ),
        0.8,
        1000,
    )
    .await;

    println!("\n=== Eval Complete ===");
}

async fn eval_step(
    backend: &RwkvBackend,
    name: &str,
    grammar: &str,
    prompt: &str,
    temperature: f32,
    max_tokens: usize,
) {
    println!(
        "Grammar (first 100 chars): {}...\n",
        &grammar[..100.min(grammar.len())]
    );

    match backend
        .complete(CompletionRequest {
            system: "You are a story generator. Respond only with valid JSON matching the grammar."
                .to_string(),
            prompt: prompt.to_string(),
            grammar: Some(grammar.to_string()),
            temperature,
            max_tokens,
            ..Default::default()
        })
        .await
    {
        Ok(resp) => {
            println!("✓ {} succeeded", name);
            println!(
                "Tokens: {} prompt, {} completion\n",
                resp.usage.prompt_tokens, resp.usage.completion_tokens
            );
            println!("Output (first 500 chars):");
            println!("{}\n", &resp.text[..500.min(resp.text.len())]);

            // Validate JSON
            match serde_json::from_str::<serde_json::Value>(&resp.text) {
                Ok(json) => {
                    println!("✓ Valid JSON");
                    println!(
                        "Structure: {}\n",
                        serde_json::to_string_pretty(&json).unwrap()
                    );
                }
                Err(e) => {
                    println!("✗ Invalid JSON: {}", e);
                    println!("Raw text: {}\n", resp.text);
                }
            }
        }
        Err(e) => {
            println!("✗ {} failed: {}", name, e);
        }
    }
}
