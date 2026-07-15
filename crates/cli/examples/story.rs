//! Story generation using the MechanisticAgent with workspace artifacts.
//!
//! Pipeline: outline → wiki → chapter (×3) → validate → synopsis → publish.
//! Each handler writes to a workspace sandbox. The agent self-corrects:
//! if output contains meta-commentary (<think>) or is too short, it retries
//! with a tightened prompt.
//!
//! Workspace artifacts produced:
//!   01-OUTLINE.md       — title, genre, tone, chapter summaries
//!   02-WIKI.md          — character bios, setting lore
//!   03-CHAPTER_1.md     — first chapter prose
//!   03-CHAPTER_2.md     — second chapter prose
//!   03-CHAPTER_3.md     — third chapter prose
//!   04-VALIDATION.md    — quality check results
//!   05-SYNOPSIS.md      — one-paragraph summary
//!   06-STORY.md         — complete published story (all chapters assembled)
//!
//! Usage:
//!   RWKV_MODEL=... cargo run --release --example story -p roco-cli \
//!     "Make me a xianxia story about a lone cultivator who levels up alone"

use std::collections::HashMap;

use roco_agent::mechanistic::{
    HandlerResult, MechanisticAgent, Plan, RepairConfig, Task,
};
use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;
use roco_workspace::{Workspace, WorkspaceKind};

/// Strip all Thinking...thinking blocks from model output.
/// Uses string search for reliable tag detection, handles both paired and open-ended blocks.
fn strip_think_blocks(text: &str) -> String {
    const OPEN: &str = "<thinking>";
    const CLOSE: &str = "</thinking>";
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;

    loop {
        if let Some(pos) = remaining.find(OPEN) {
            // Copy everything before this think block
            result.push_str(&remaining[..pos]);
            // Skip past the open tag
            remaining = &remaining[pos + OPEN.len()..];
            // Find matching close tag
            if let Some(close_pos) = remaining.find(CLOSE) {
                // Discard think content, resume after close tag
                remaining = &remaining[close_pos + CLOSE.len()..];
            } else {
                // No closing tag found — entire rest is think content, discard it
                break;
            }
        } else {
            // No more think blocks — copy remainder
            result.push_str(remaining);
            break;
        }
    }

    // Collapse multiple spaces/newlines into single space, trim
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}


/// Detect if model output is contaminated with meta-commentary.
fn has_meta_contamination(text: &str) -> bool {
    text.contains("<think>")
        || text.contains("</think>")
        || text.contains("We need to")
        || text.contains("I'll write")
        || text.contains("Let's craft")
        || text.contains("Write 300 words")
        || text.contains("We'll write")
        || text.contains("Start with")
        || text.contains("~300 words")
        || text.len() < 50
}

/// Self-correcting model call: retry with tightened prompt if output is poor.
fn clean_complete(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    temperature: f32,
    max_tokens: usize,
    label: &str,
) -> Result<String, String> {
    let mut attempt = 0;
    let mut temp = temperature;
    let mut tweak = String::new();

    loop {
        let full_prompt = if attempt == 0 {
            prompt.to_string()
        } else {
            format!(
                "{}\n\nIMPORTANT: Write DIRECTLY. No thinking, no planning, no <think> tags.\n\
                 Just output the content itself.\n{}",
                prompt, tweak
            )
        };

        let resp = futures::executor::block_on(backend.complete(CompletionRequest {
            system: format!(
                "{} You output ONLY the requested content with NO meta-commentary, no <think> tags, no planning text.",
                system
            ),
            prompt: full_prompt,
            temperature: temp,
            max_tokens,
            ..Default::default()
        }))
        .map_err(|e| format!("model error: {e}"))?;

        let text = resp.text;

        if !has_meta_contamination(&text) {
            return Ok(text);
        }

        attempt += 1;
        if attempt >= 3 {
            // Strip ALL <think>...<\think> blocks and return remaining prose.
            let stripped = strip_think_blocks(&text).trim().to_string();
            if !stripped.is_empty() {
                return Ok(stripped);
            }
            return Err(format!("{}: failed to extract usable output after {attempt} attempts", label));
        }

        temp = (temp - 0.2).max(0.3);
        tweak = if text.contains("<think>") {
            "Your last response contained <think> tags. Output ONLY the final content, nothing else.".to_string()
        } else if text.len() < 100 {
            "Your response was too short. Write at least one full paragraph.".to_string()
        } else {
            "Write directly. No meta-commentary.".to_string()
        };
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    let prompt = std::env::args().nth(1).unwrap_or_else(|| {
        "Write a short story about a lighthouse keeper who discovers a message in a bottle.".to_string()
    });

    println!("Loading model...");
    let backend = RwkvBackend::from_env()?;
    println!("Model ready. Generating story...\n");

    // Build the mechanistic agent with the storyTeller route.
    let mut agent = MechanisticAgent::new()
        .with_repair(RepairConfig {
            max_retries: 2,
            temperature: 0.7,
            temperature_delta: 0.2,
            temperature_floor: 0.3,
            max_tokens: 512,
            token_decay: 128,
            min_tokens: 128,
        })
        .with_fallback_threshold(0.3);

    agent.add_route("storyTeller", vec![
        ("compose", "outline"),
        ("compose", "wiki"),
        ("write", "chapter"),
        ("write", "synopsis"),
        ("validate", "chapter"),
        ("publish", "chapter"),
    ]);

    // ── compose/outline ──────────────────────────────────────────────
    agent.register("compose", "outline", Box::new(|task, backend, ws| {
        let premise = task.spec.get("premise")
            .and_then(|v| v.as_str())
            .unwrap_or("a short story");
        let out = clean_complete(
            backend, "You are a story outliner.",
            &format!(
                "Outline a short story with 3 chapters based on this premise:\n{premise}\n\n\
                 Output:\nTitle: ...\nGenre: ...\nTone: ...\n\
                 Chapter 1: ...\nChapter 2: ...\nChapter 3: ...\n\n\
                 Just the outline, nothing else.",
            ),
            0.6, 300, "outline",
        ).unwrap_or_else(|e| format!("[outline error: {e}]"));
        let path = ws.resolve("01-OUTLINE.md").unwrap();
        std::fs::write(&path, &out).ok();
        HandlerResult {
            task: task.clone(),
            output: out,
            files: HashMap::new(),
        }
    }));

    // ── compose/wiki ────────────────────────────────────────────────
    agent.register("compose", "wiki", Box::new(|task, backend, ws| {
        let premise = task.spec.get("premise")
            .and_then(|v| v.as_str())
            .unwrap_or("a short story");
        let outline = task.spec.get("outline")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let out = clean_complete(
            backend, "You are a worldbuilding assistant.",
            &format!(
                "Based on this premise and outline, create character bios and setting lore:\n\n\
                 Premise: {premise}\nOutline: {outline}\n\n\
                 Characters:\n  - [name]: [description]\n  - ...\n\n\
                 Setting:\n  - [location]: [description]\n  - ...\n\n\
                 Just the wiki content, nothing else.",
            ),
            0.7, 400, "wiki",
        ).unwrap_or_else(|e| format!("[wiki error: {e}]"));
        let path = ws.resolve("02-WIKI.md").unwrap();
        std::fs::write(&path, &out).ok();
        HandlerResult {
            task: task.clone(),
            output: out,
            files: HashMap::new(),
        }
    }));

    // ── write/chapter ────────────────────────────────────────────────
    agent.register("write", "chapter", Box::new(|task, backend, ws| {
        let chapter_num: usize = task.spec.get("number")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;
        let chapter_label = task.spec.get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("Chapter 1");
        let outline = task.spec.get("outline")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let previous = task.spec.get("previous")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let directive = if chapter_num == 1 {
            format!(
                "Write {chapter_label}. Introduce the main character and setting. ~400 words.\n\n\
                 Outline context:\n{outline}",
            )
        } else {
            format!(
                "Write {chapter_label}. Continue from where the previous chapter left off. \
                 Advance the plot. ~400 words.\n\n\
                 Previous chapter recap:\n{previous}\n\n\
                 Outline context:\n{outline}",
            )
        };

        let out = clean_complete(
            backend, "You are a fiction writer. Write vivid, engaging prose.",
            &directive,
            0.8, 600, &format!("chapter {chapter_num}"),
        ).unwrap_or_else(|e| format!("[chapter {chapter_num} error: {e}]"));

        let filename = format!("03-CHAPTER_{}.md", chapter_num);
        let path = ws.resolve(&filename).unwrap();
        std::fs::write(&path, &out).ok();
        HandlerResult {
            task: task.clone(),
            output: out,
            files: HashMap::new(),
        }
    }));

    // ── validate/chapter ────────────────────────────────────────────
    agent.register("validate", "chapter", Box::new(|task, backend, ws| {
        let chapter_text = task.spec.get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let chapter_num = task.spec.get("number")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if chapter_text.trim().is_empty() {
            let note = format!("[validation skipped — chapter {chapter_num} is empty]");
            let path = ws.resolve("04-VALIDATION.md").unwrap();
            std::fs::write(&path, &note).ok();
            return HandlerResult {
                task: task.clone(),
                output: note,
                files: HashMap::new(),
            };
        }

        let prompt = format!(
            "Review this chapter and check for:\n\
             1. Does it read like a coherent story (not meta-commentary)?\n\
             2. Are there any <think> tags or planning text?\n\
             3. Is the prose engaging?\n\n\
             Chapter:\n{chapter_text}\n\n\
             Output only:\n\
             Quality: pass | fail | needs-work\n\
             Issues: ...\n\
             Suggestion: ...",
        );

        let out = futures::executor::block_on(backend.complete(CompletionRequest {
            system: "You are a quality reviewer. Be strict. Output only the requested format.".into(),
            prompt,
            temperature: 0.3,
            max_tokens: 200,
            ..Default::default()
        })).map(|r| r.text).unwrap_or_else(|_| "Quality: fail\nIssues: model error".to_string());

        // Append to VALIDATION.md (accumulate across chapters)
        let path = ws.resolve("04-VALIDATION.md").unwrap();
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let entry = format!("\n## Chapter {chapter_num}\n{out}\n");
        std::fs::write(&path, existing + &entry).ok();

        HandlerResult {
            task: task.clone(),
            output: out,
            files: HashMap::new(),
        }
    }));

    // ── write/synopsis ──────────────────────────────────────────────
    agent.register("write", "synopsis", Box::new(|task, backend, ws| {
        let chapters = task.spec.get("chapters")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let out = clean_complete(
            backend, "You are a literary summarizer.",
            &format!(
                "Write a one-paragraph synopsis of the complete story based on these chapters:\n\n\
                 {chapters}\n\nSynopsis (one paragraph, ~100 words):",
            ),
            0.5, 200, "synopsis",
        ).unwrap_or_else(|e| format!("[synopsis error: {e}]"));
        let path = ws.resolve("05-SYNOPSIS.md").unwrap();
        std::fs::write(&path, &out).ok();
        HandlerResult {
            task: task.clone(),
            output: out,
            files: HashMap::new(),
        }
    }));

    // ── publish/chapter ────────────────────────────────────────────
    agent.register("publish", "chapter", Box::new(|_task, _backend, ws| {
        // Assemble all chapters into a single STORY.md
        let outline = std::fs::read_to_string(ws.root().join("01-OUTLINE.md")).unwrap_or_default();
        let wiki = std::fs::read_to_string(ws.root().join("02-WIKI.md")).unwrap_or_default();
        let mut story = format!("# {}\n\n", extract_title(&outline));

        if !wiki.is_empty() {
            story.push_str("## Characters & Setting\n\n");
            story.push_str(&wiki);
            story.push_str("\n\n---\n\n");
        }

        for i in 1..=3 {
            let ch = std::fs::read_to_string(ws.root().join(&format!("03-CHAPTER_{i}.md")))
                .unwrap_or_default();
            if !ch.is_empty() {
                story.push_str(&ch);
                story.push_str("\n\n---\n\n");
            }
        }

        let synopsis = std::fs::read_to_string(ws.root().join("05-SYNOPSIS.md"))
            .unwrap_or_default();
        if !synopsis.is_empty() {
            story.push_str("## Synopsis\n\n");
            story.push_str(&synopsis);
            story.push_str("\n");
        }

        let path = ws.resolve("06-STORY.md").unwrap();
        std::fs::write(&path, &story).ok();

        HandlerResult {
            task: Task {
                r#type: "publish".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({"status": "published"}),
            },
            output: format!("published {} bytes", story.len()),
            files: HashMap::new(),
        }
    }));

    // Build a hand-crafted plan since the model can't reliably output JSON.
    let plan = Plan {
        tasks: vec![
            // Step 1: outline
            Task {
                r#type: "compose".into(),
                domain: "outline".into(),
                spec: serde_json::json!({"premise": prompt}),
            },
        ],
    };

    let ws = create_workspace(&prompt)?;
    let workspace_path = ws.root().to_string_lossy().to_string();

    println!("\nWorkspace: {workspace_path}\n");
    println!("Pipeline: outline → wiki → chapter×3 → validate → synopsis → publish\n");

    // Phase 1: outline
    println!("📝 Outline...");
    let outline_result = agent.dispatch_single(&backend, &plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("outline failed: {e}"))?;
    let outline_text = &outline_result.output;

    // Phase 2: wiki
    println!("📚 Worldbuilding...");
    let wiki_plan = Plan {
        tasks: vec![Task {
            r#type: "compose".into(),
            domain: "wiki".into(),
            spec: serde_json::json!({"premise": prompt, "outline": outline_text}),
        }],
    };
    let wiki_result = agent.dispatch_single(&backend, &wiki_plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("wiki failed: {e}"))?;

    // Phase 3: chapters ×3
    let mut chapter_texts = Vec::new();
    for i in 1..=3 {
        let chapter_label = format!("Chapter {i}");
        let previous = chapter_texts.last().cloned().unwrap_or_default();
        println!("✍️  {}...", &chapter_label);

        // Write chapter
        let ch_task = Task {
            r#type: "write".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({
                "number": i,
                "label": chapter_label,
                "outline": outline_text,
                "previous": previous,
            }),
        };
        let ch_result = agent.dispatch_single(&backend, &ch_task, &ws)
            .map_err(|e| anyhow::anyhow!("chapter {i} failed: {e}"))?;
        chapter_texts.push(ch_result.output.clone());

        // Validate chapter
        println!("🔍 Validating {}...", &chapter_label);
        let val_task = Task {
            r#type: "validate".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({
                "number": i,
                "text": ch_result.output,
            }),
        };
        let _val_result = agent.dispatch_single(&backend, &val_task, &ws)
            .map_err(|e| anyhow::anyhow!("validation {i} failed: {e}"))?;

        // If validation found issues, re-run with corrected prompt
        // (self-correction loop)
        let val_path = ws.root().join("04-VALIDATION.md");
        if let Ok(val_content) = std::fs::read_to_string(&val_path) {
            if val_content.contains("Quality: fail") || val_content.contains("needs-work") {
                println!("⚠️  {} needs revision — retrying with corrections...", &chapter_label);
                let corrected = clean_complete(
                    &backend,
                    "You are a fiction writer. Write vivid, engaging prose. NO meta-commentary.",
                    &format!(
                        "Rewrite {chapter_label}. The previous version had quality issues.\n\n\
                         Previous version:\n{}\n\n\
                         Write a better version. ~400 words. JUST the story text.",
                        ch_result.output
                    ),
                    0.7, 600, &format!("chapter {i} retry"),
                ).unwrap_or_else(|_| ch_result.output.clone());

                // Overwrite the chapter with corrected version
                let filename = format!("03-CHAPTER_{}.md", i);
                let path = ws.resolve(&filename).unwrap();
                std::fs::write(&path, &corrected).ok();
                chapter_texts[i - 1] = corrected;
            }
        }
    }

    // Phase 4: synopsis
    println!("📋 Synopsis...");
    let all_chapters = chapter_texts.iter()
        .enumerate()
        .map(|(i, t)| format!("## Chapter {}\n{}", i + 1, t))
        .collect::<Vec<_>>()
        .join("\n\n");
    let synopsis_task = Task {
        r#type: "write".into(),
        domain: "synopsis".into(),
        spec: serde_json::json!({"chapters": all_chapters}),
    };
    let _synopsis_result = agent.dispatch_single(&backend, &synopsis_task, &ws)
        .map_err(|e| anyhow::anyhow!("synopsis failed: {e}"))?;

    // Phase 5: publish
    println!("📦 Publishing...");
    let publish_task = Task {
        r#type: "publish".into(),
        domain: "chapter".into(),
        spec: serde_json::json!({}),
    };
    let publish_result = agent.dispatch_single(&backend, &publish_task, &ws)
        .map_err(|e| anyhow::anyhow!("publish failed: {e}"))?;

    // Snapshot the workspace for display
    let outcome = agent.commit(plan.clone(), vec![
        outline_result, wiki_result, publish_result,
    ], &ws)?;

    println!("✅ Done! {} files in workspace:\n", outcome.workspace_files.len());
    let mut filenames: Vec<_> = outcome.workspace_files.keys().collect();
    filenames.sort();
    for fname in &filenames {
        let size = outcome.workspace_files[*fname].len();
        let preview: String = outcome.workspace_files[*fname]
            .lines()
            .take(5)
            .collect::<Vec<_>>()
            .join("\n");
        println!("  📄 {} ({} bytes)", fname, size);
        for line in preview.lines() {
            println!("     {}", line);
        }
        println!();
    }

    println!("Workspace path: {}", outcome.workspace_path);
    println!("\nStory published to 06-STORY.md inside the workspace.");
    Ok(())
}

/// Sanitize a string into a safe filesystem directory name.
fn sanitize_dirname(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
}

/// Create a unique persistent workspace under `.roco/workspaces/story_<sanitized_prompt>_<ts>/`.
/// Appends a timestamp to prevent collisions when running the same prompt multiple times.
fn create_workspace(prompt: &str) -> Result<Workspace, anyhow::Error> {
    let base = std::env::current_dir()?.join(".roco").join("workspaces");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = if prompt.trim().is_empty() {
        format!("story_ts_{ts}")
    } else {
        let words: Vec<&str> = prompt.split_whitespace().take(4).collect();
        format!("story_{}", sanitize_dirname(&words.join("_")))
    };
    let dir = base.join(format!("{name}_{ts}"));
    std::fs::create_dir_all(&dir)?;
    let ws = Workspace::from_existing(dir, WorkspaceKind::Agent)?;
    Ok(ws.with_name(name))
}

/// Extract the title from an outline markdown file.
fn extract_title(outline: &str) -> String {
    for line in outline.lines() {
        if line.starts_with("Title:") {
            return line.trim_start_matches("Title:").trim().to_string();
        }
    }
    "Untitled Story".to_string()
}
