//! Story pilot — mechanistic agent demo for story generation.
//!
//! Demonstrates the proper mechanistic agent pattern:
//! - Input → classify intent → generate structured Plan (JSON)
//! - Loop through tasks, dispatch to registered handlers
//! - Each handler executes and can trigger thinking/planning
//! - Check results with inference between steps
//!
//! Usage:
//!   RWKV_MODEL=... RWKV_VOCAB=... \
//!   cargo run --release --example story_pilot -p roco-cli -- "write a story about..."

use std::collections::HashMap;
use std::path::PathBuf;

use roco_agent::mechanistic::{HandlerResult, MechanisticAgent, Task};
use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;
use roco_workspace::Workspace;
use serde_json::Value;
use tracing::{error, info};

/// Helper: call model with raw prompt, return text
fn complete_raw(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    temperature: f32,
    max_tokens: usize,
) -> anyhow::Result<String> {
    let req = CompletionRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        output_schema: None,
        grammar: None,
        temperature,
        max_tokens,
        estimated_prompt_tokens: 0,
        thinking: false,
        preserve_state: false,
        on_token: None,
        session: None,
    };
    let resp = futures::executor::block_on(backend.complete(req))?;
    Ok(resp.text)
}

/// Handler: create story outline
fn handle_outline(
    task: &Task,
    backend: &dyn ModelBackend,
    _ws: &Workspace,
) -> HandlerResult {
    let premise = task.spec.get("premise").and_then(|v| v.as_str()).unwrap_or("");
    info!(%premise, "generating outline");

    let prompt = format!(
        "Create a 3-chapter story outline based on: {premise}\n\nReturn JSON: {{\"title\":str,\"genre\":str,\"tone\":str,\"chapter1\":str,\"chapter2\":str,\"chapter3\":str}}"
    );
    let raw = complete_raw(backend, "You are a story outliner.", &prompt, 0.6, 400)
        .unwrap_or_else(|e| {
            error!(error = %e, "outline generation failed");
            String::new()
        });
    let outline = extract_json(&raw).unwrap_or_else(|| Value::Null);

    HandlerResult {
        task: task.clone(),
        output: serde_json::to_string(&outline).unwrap_or_default(),
        files: HashMap::new(),
    }
}

/// Handler: create wiki
fn handle_wiki(
    task: &Task,
    backend: &dyn ModelBackend,
    _ws: &Workspace,
) -> HandlerResult {
    let outline = task.spec.get("outline").and_then(|v| v.as_str()).unwrap_or("");
    info!("generating wiki");

    let prompt = format!(
        "For this story outline, create one main character and one setting:\n{outline}\n\nReturn JSON: {{\"name\":str,\"description\":str,\"setting\":str,\"setting_desc\":str}}"
    );
    let raw = complete_raw(backend, "You are a worldbuilding assistant.", &prompt, 0.6, 300)
        .unwrap_or_else(|e| {
            error!(error = %e, "wiki generation failed");
            String::new()
        });
    let wiki = extract_json(&raw).unwrap_or_else(|| Value::Null);

    HandlerResult {
        task: task.clone(),
        output: serde_json::to_string(&wiki).unwrap_or_default(),
        files: HashMap::new(),
    }
}

/// Handler: plan chapter
fn handle_plan_chapter(
    task: &Task,
    backend: &dyn ModelBackend,
    _ws: &Workspace,
) -> HandlerResult {
    let chapter_num = task.spec.get("chapter_num").and_then(|v| v.as_u64()).unwrap_or(1);
    let summary = task.spec.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let outline = task.spec.get("outline").and_then(|v| v.as_str()).unwrap_or("");
    info!(chapter = chapter_num, "planning chapter");

    let prompt = format!(
        "Plan chapter {chapter_num} of this story.\nOutline: {outline}\nChapter summary: {summary}\n\nReturn JSON: {{\"plan\":str,\"focus\":str,\"pacing\":str}}"
    );
    let raw = complete_raw(backend, "You are a story planner.", &prompt, 0.5, 250)
        .unwrap_or_else(|e| {
            error!(error = %e, "plan chapter failed");
            String::new()
        });
    let plan = extract_json(&raw).unwrap_or_else(|| Value::Null);

    HandlerResult {
        task: task.clone(),
        output: serde_json::to_string(&plan).unwrap_or_default(),
        files: HashMap::new(),
    }
}

/// Handler: write chapter
fn handle_write_chapter(
    task: &Task,
    backend: &dyn ModelBackend,
    _ws: &Workspace,
) -> HandlerResult {
    let chapter_num = task.spec.get("chapter_num").and_then(|v| v.as_u64()).unwrap_or(1);
    let plan = task.spec.get("plan").and_then(|v| v.as_str()).unwrap_or("");
    let prev = task.spec.get("prev_chapter").and_then(|v| v.as_str()).unwrap_or("");
    info!(chapter = chapter_num, "writing chapter");

    let context = if prev.is_empty() {
        format!("Chapter plan: {plan}")
    } else {
        format!("Previous chapter ending: {prev}\n\nThis chapter plan: {plan}")
    };

    let prompt = format!(
        "{context}\n\nWrite chapter {chapter_num}. ~300 words of vivid prose. Output only the narrative."
    );
    let prose = complete_raw(backend, "You are a fiction writer.", &prompt, 0.8, 500)
        .unwrap_or_else(|e| {
            error!(error = %e, "write chapter failed");
            String::new()
        });

    HandlerResult {
        task: task.clone(),
        output: prose,
        files: HashMap::new(),
    }
}

/// Handler: evaluate chapter
fn handle_evaluate(
    task: &Task,
    backend: &dyn ModelBackend,
    _ws: &Workspace,
) -> HandlerResult {
    let chapter = task.spec.get("chapter").and_then(|v| v.as_str()).unwrap_or("");
    let plan = task.spec.get("plan").and_then(|v| v.as_str()).unwrap_or("");
    info!("evaluating chapter");

    let prompt = format!(
        "Evaluate this chapter against its plan.\nPlan: {plan}\n\nChapter:\n{chapter}\n\nReturn JSON: {{\"quality\":str,\"issues\":str}}"
    );
    let raw = complete_raw(backend, "You are a strict editor.", &prompt, 0.4, 200)
        .unwrap_or_else(|e| {
            error!(error = %e, "evaluate chapter failed");
            String::new()
        });
    let eval_result = extract_json(&raw).unwrap_or_else(|| Value::Null);

    HandlerResult {
        task: task.clone(),
        output: serde_json::to_string(&eval_result).unwrap_or_default(),
        files: HashMap::new(),
    }
}

/// Handler: edit chapter
fn handle_edit(
    task: &Task,
    backend: &dyn ModelBackend,
    _ws: &Workspace,
) -> HandlerResult {
    let chapter = task.spec.get("chapter").and_then(|v| v.as_str()).unwrap_or("");
    let feedback = task.spec.get("feedback").and_then(|v| v.as_str()).unwrap_or("");
    info!("editing chapter");

    let prompt = format!(
        "Revise this chapter.\nFeedback: {feedback}\n\nChapter:\n{chapter}\n\nReturn the improved chapter as plain text."
    );
    let revised = complete_raw(backend, "You are an editor.", &prompt, 0.7, 500)
        .unwrap_or_else(|e| {
            error!(error = %e, "edit chapter failed");
            String::new()
        });

    HandlerResult {
        task: task.clone(),
        output: revised,
        files: HashMap::new(),
    }
}

/// Extract first JSON object from raw text
fn extract_json(raw: &str) -> Option<Value> {
    let start = raw.find('{')?;
    let mut depth = 0;
    let mut in_str = false;
    let mut esc = false;
    let bytes = raw.as_bytes();
    let mut end = None;

    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            b'"' => in_str = true,
            _ => {}
        }
    }

    let end = end?;
    let slice = &raw[start..=end];
    serde_json::from_str(slice).ok()
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let premise = std::env::args().nth(1).unwrap_or_else(|| {
        "a lighthouse keeper who discovers a message in a bottle".to_string()
    });

    info!(%premise, "story pilot starting");

    info!("loading model");
    let backend = RwkvBackend::from_env()?;
    info!("model ready");

    // Create mechanistic agent
    let mut agent = MechanisticAgent::new(&backend);

    // Register handlers for story generation tasks
    // The agent will generate the plan dynamically via structured output
    agent.register("write", "outline", Box::new(handle_outline));
    agent.register("write", "wiki", Box::new(handle_wiki));
    agent.register("plan", "chapter", Box::new(handle_plan_chapter));
    agent.register("write", "chapter", Box::new(handle_write_chapter));
    agent.register("validate", "chapter", Box::new(handle_evaluate));
    agent.register("edit", "chapter", Box::new(handle_edit));

    info!("handlers registered, running mechanistic agent");

    // Run the mechanistic agent
    // It will:
    // 1. Classify intent → determine mode (storyTeller)
    // 2. Think about the request
    // 3. Derive a structured Plan (JSON with tasks)
    // 4. Dispatch tasks to registered handlers in a loop
    // 5. Commit results to workspace
    let result = agent.run(&premise)?;

    info!(
        num_tasks = result.plan.tasks.len(),
        num_results = result.handler_results.len(),
        "story generation complete"
    );

    // Write the story from workspace files
    let mut story = String::new();
    
    // Collect all markdown files from workspace
    for (path, content) in &result.workspace_files {
        if path.ends_with(".md") {
            story.push_str(&format!("## {}\n\n{}\n\n", path, content));
        }
    }

    // Write final story
    let story_path = PathBuf::from(".roco/story/workspace/STORY.md");
    std::fs::create_dir_all(story_path.parent().unwrap())?;
    std::fs::write(&story_path, &story)?;
    
    info!(path = %story_path.display(), "story written");

    Ok(())
}
