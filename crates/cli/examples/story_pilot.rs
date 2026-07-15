//! Story Pilot — MechanisticAgent-driven story generation.
//!
//! Demonstrates the MechanisticAgent pattern:
//! - Register routes and handlers for a "storyTeller" workflow
//! - Build a structured Plan in code (reliable)
//! - Dispatch tasks → handlers write to sandboxed workspace
//! - Commit → snapshot all workspace files
//!
//! Output lands in `.roco/workspaces/story_<prompt>_<ts>/`.
//!
//! Usage:
//!   cargo run --release --example story_pilot -p roco-cli \
//!     "Make me a xianxia story about a lone cultivator who levels up alone"

use std::collections::HashMap;
use std::time::SystemTime;

use roco_agent::mechanistic::{
    HandlerResult, MechanisticAgent, Plan, RepairConfig, Task,
};
use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;
use roco_workspace::{Workspace, WorkspaceKind};
use tracing::{error, info};

// ── Helpers ──────────────────────────────────────────────────────────

/// Detect meta-contamination in model output.
fn has_meta(text: &str) -> bool {
    text.contains("<think>")
        || text.contains("</think>")
        || text.contains("We need to")
        || text.len() < 30
}

/// Clean-complete: retry until model output is free of meta-commentary.
fn clean_complete(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    temperature: f32,
    max_tokens: usize,
    label: &str,
) -> anyhow::Result<String> {
    let mut attempt = 0u32;
    let mut temp = temperature;
    let mut tweak = String::new();

    loop {
        let resp = futures::executor::block_on(backend.complete(CompletionRequest {
            system: format!(
                "{} You output ONLY the requested content. No thinking, no 1<think> tags.",
                system
            ),
            prompt: if attempt == 0 {
                prompt.to_string()
            } else {
                format!(
                    "{prompt}\n\nIMPORTANT: Write DIRECTLY. NO thinking or planning.\n{tweak}",
                )
            },
            temperature: temp,
            max_tokens,
            ..Default::default()
        }))
        .map_err(|e| anyhow::anyhow!("{label}: {e}"))?;

        let text = resp.text;
        if !has_meta(&text) {
            return Ok(text);
        }

        attempt += 1;
        if attempt >= 3 {
            // Last resort: strip <think> blocks.
            let cleaned = text
                .split("thi nk>")
                .flat_map(|s| s.split("</think>"))
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string();
            if cleaned.len() > 30 {
                return Ok(cleaned);
            }
            return Err(anyhow::anyhow!("{label}: model produced no clean output after {attempt} retries"));
        }
        temp = (temp - 0.2).max(0.3);
        tweak = if text.contains("think>") {
            "Your response contained 1<think> tags. Output ONLY the final content.".into()
        } else if text.len() < 60 {
            "Your response was too short. Write at least one full paragraph.".into()
        } else {
            "Write directly. No meta-commentary.".into()
        };
    }
}

// ── Handlers ────────────────────────────────────────────────────────

fn handle_outline(task: &Task, backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let premise = task.spec.get("premise").and_then(|v| v.as_str()).unwrap_or("");
    info!(premise = %premise, "generate outline");

    let out = match clean_complete(
        backend, "You are a story outliner.",
        &format!(
            "Outline a 3-chapter story from:\n{premise}\n\nTitle: ...\nGenre: ...\nTone: ...\nCh1: ...\nCh2: ...\nCh3: ...",
        ),
        0.6, 350, "outline",
    ) {
        Ok(t) => t,
        Err(e) => { error!(%e); "[outline error]".into() }
    };

    // Save to workspace
    let path = ws.resolve("01-OUTLINE.md");
    if let Ok(p) = &path {
        std::fs::write(p, &out).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: out.clone(),
        files: HashMap::from([("01-OUTLINE.md".into(), out)]),
    }
}

fn handle_wiki(task: &Task, backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let premise = task.spec.get("premise").and_then(|v| v.as_str()).unwrap_or("");
    let outline = task.spec.get("outline").and_then(|v| v.as_str()).unwrap_or("");
    info!("generate wiki");

    let out = match clean_complete(
        backend, "You are a worldbuilding assistant.",
        &format!(
            "Create characters and setting lore for this story:\nPremise: {premise}\nOutline: {outline}\n\nCharacters:\n  [name]: [desc]\nSetting:\n  [loc]: [desc]",
        ),
        0.7, 450, "wiki",
    ) {
        Ok(t) => t,
        Err(e) => { error!(%e); "[wiki error]".into() }
    };

    let path = ws.resolve("02-WIKI.md");
    if let Ok(p) = &path {
        std::fs::write(p, &out).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: out.clone(),
        files: HashMap::from([("02-WIKI.md".into(), out)]),
    }
}

fn handle_chapter(task: &Task, backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let num = task.spec.get("number").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let outline = task.spec.get("outline").and_then(|v| v.as_str()).unwrap_or("");
    let prev = task.spec.get("previous").and_then(|v| v.as_str()).unwrap_or("");

    let directive = if num == 1 {
        format!("Write Chapter 1. Introduce character and setting. ~400 words.\nOutline:\n{outline}")
    } else {
        format!("Write Chapter {num}. Continue from previous. Advance plot. ~400 words.\nPrevious ending:\n{prev}\n\nOutline:\n{outline}")
    };

    let out = match clean_complete(
        backend, "You are a fiction writer. Write vivid, engaging prose.",
        &directive, 0.8, 650, &format!("chapter {num}"),
    ) {
        Ok(t) => t,
        Err(e) => { error!(%e); format!("[chapter {num} error]") }
    };

    let fname = format!("03-CHAPTER_{num}.md");
    let path = ws.resolve(&fname);
    if let Ok(p) = &path {
        std::fs::write(p, &out).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: out.clone(),
        files: HashMap::from([(fname, out)]),
    }
}

fn handle_validate(task: &Task, _backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let num = task.spec.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
    let text = task.spec.get("text").and_then(|v| v.as_str()).unwrap_or("");
    info!(chapter = num, "validate chapter");

    let note = if text.trim().is_empty() {
        format!("[chapter {num} skipped — empty]")
    } else {
        format!("Chapter {num}: OK (length: {} chars)", text.len())
    };

    // Append validation to running file
    let path = ws.resolve("04-VALIDATION.md");
    if let Ok(p) = path {
        let existing = std::fs::read_to_string(&p).unwrap_or_default();
        std::fs::write(&p, format!("{existing}\n## Ch{num}\n{note}\n")).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: note,
        files: HashMap::new(),
    }
}

fn handle_synopsis(task: &Task, backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let chapters = task.spec.get("chapters").and_then(|v| v.as_str()).unwrap_or("");
    info!("generate synopsis");

    let out = match clean_complete(
        backend, "You are a literary summarizer.",
        &format!("One-paragraph synopsis (~100 words):\n\n{chapters}"),
        0.5, 250, "synopsis",
    ) {
        Ok(t) => t,
        Err(e) => { error!(%e); "[synopsis error]".into() }
    };

    let path = ws.resolve("05-SYNOPSIS.md");
    if let Ok(p) = &path {
        std::fs::write(p, &out).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: out.clone(),
        files: HashMap::from([("05-SYNOPSIS.md".into(), out)]),
    }
}

fn handle_publish(_task: &Task, _backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    info!("assemble final story");

    let outline = std::fs::read_to_string(ws.root().join("01-OUTLINE.md")).unwrap_or_default();
    let wiki = std::fs::read_to_string(ws.root().join("02-WIKI.md")).unwrap_or_default();
    let synopsis = std::fs::read_to_string(ws.root().join("05-SYNOPSIS.md")).unwrap_or_default();

    // Extract title from outline
    let title = outline.lines()
        .find(|l| l.starts_with("Title:") || l.starts_with("# "))
        .map(|l| l.replace("Title:", "").replace("# ", "").trim().to_string())
        .unwrap_or_else(|| "Untitled Story".into());

    // Assemble
    let mut story = format!("# {title}\n\n");
    if !wiki.is_empty() {
        story.push_str("## Characters & Setting\n\n");
        story.push_str(&wiki);
        story.push_str("\n\n---\n\n");
    }

    for i in 1..=3 {
        let ch = std::fs::read_to_string(ws.root().join(format!("03-CHAPTER_{i}.md")))
            .unwrap_or_default();
        if !ch.is_empty() {
            story.push_str(&ch);
            story.push_str("\n\n---\n\n");
        }
    }

    if !synopsis.is_empty() {
        story.push_str("## Synopsis\n\n");
        story.push_str(&synopsis);
    }

    let path = ws.resolve("06-STORY.md");
    if let Ok(p) = &path {
        std::fs::write(p, &story).ok();
    }

    HandlerResult {
        task: _task.clone(),
        output: format!("published {} bytes", story.len()),
        files: HashMap::from([("06-STORY.md".into(), story)]),
    }
}

// ── Utilities ───────────────────────────────────────────────────────

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
}

fn create_workspace(prompt: &str) -> anyhow::Result<Workspace> {
    let ts = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default().as_secs();
    let words: Vec<&str> = prompt.split_whitespace().take(4).collect();
    let name = if words.is_empty() {
        format!("story_ts_{ts}")
    } else {
        format!("story_{}", sanitize(&words.join("_")))
    };
    let dir = std::env::current_dir()?
        .join(".roco")
        .join("workspaces")
        .join(format!("{name}_{ts}"));
    std::fs::create_dir_all(&dir)?;
    let ws = Workspace::from_existing(dir, WorkspaceKind::Agent)?;
    Ok(ws.with_name(name))
}

// ── Main ────────────────────────────────────────────────────────────

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    let premise = std::env::args().nth(1).unwrap_or_else(|| {
        "Write a story about a lone cultivator who discovers a hidden inheritance".into()
    });

    info!(%premise, "StoryPilot starting");
    println!("Loading model...\n");
    let backend = RwkvBackend::from_env()?;
    println!("Model ready.\n");

    // ── Build MechanisticAgent ──────────────────────────────────────
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

    // Declare the storyTeller route
    agent.add_route("storyTeller", vec![
        ("compose", "outline"),
        ("compose", "wiki"),
        ("write", "chapter"),
        ("validate", "chapter"),
        ("write", "synopsis"),
        ("publish", "chapter"),
    ]);

    // Register handlers
    agent.register("compose", "outline", Box::new(handle_outline));
    agent.register("compose", "wiki", Box::new(handle_wiki));
    agent.register("write", "chapter", Box::new(handle_chapter));
    agent.register("validate", "chapter", Box::new(handle_validate));
    agent.register("write", "synopsis", Box::new(handle_synopsis));
    agent.register("publish", "chapter", Box::new(handle_publish));

    // ── Create workspace ───────────────────────────────────────────
    let ws = create_workspace(&premise)?;
    let workspace_path = ws.root().to_string_lossy().to_string();

    println!("Pipeline: outline → wiki → chapter×3 → validate → synopsis → publish\n");

    // ── Phase 1: Outline ───────────────────────────────────────────
    println!("📝 Outline...");
    let outline_plan = Plan {
        tasks: vec![Task {
            r#type: "compose".into(),
            domain: "outline".into(),
            spec: serde_json::json!({"premise": premise}),
        }],
    };
    let outline_result = agent.dispatch_single(&backend, &outline_plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("outline failed: {e}"))?;
    let outline_text = outline_result.output.clone();

    // ── Phase 2: Wiki ──────────────────────────────────────────────
    println!("📚 Worldbuilding...");
    let wiki_plan = Plan {
        tasks: vec![Task {
            r#type: "compose".into(),
            domain: "wiki".into(),
            spec: serde_json::json!({"premise": &premise, "outline": &outline_text}),
        }],
    };
    let wiki_result = agent.dispatch_single(&backend, &wiki_plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("wiki failed: {e}"))?;

    // ── Phase 3: Chapters ×3 ───────────────────────────────────────
    let mut chapter_texts = Vec::new();
    for i in 1..=3 {
        let label = format!("Chapter {i}");
        let prev = chapter_texts.last().cloned().unwrap_or_default();
        println!("✍️  {label}...");

        let ch_task = Task {
            r#type: "write".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({
                "number": i,
                "outline": &outline_text,
                "previous": prev,
            }),
        };
        let ch_result = agent.dispatch_single(&backend, &ch_task, &ws)
            .map_err(|e| anyhow::anyhow!("chapter {i} failed: {e}"))?;
        chapter_texts.push(ch_result.output.clone());

        // Validate
        println!("   🔍 Validating...");
        let val_task = Task {
            r#type: "validate".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({"number": i, "text": ch_result.output}),
        };
        let _val = agent.dispatch_single(&backend, &val_task, &ws)
            .map_err(|e| anyhow::anyhow!("validation {i} failed: {e}"))?;

        // Self-correction: if chapter is very short, re-write
        if ch_result.output.len() < 100 {
            println!("   ⚠️  Short chapter — retrying...");
            let corrected = clean_complete(
                &backend,
                "You are a fiction writer. Write vivid prose. NO meta-commentary.",
                &format!("Rewrite {label}. Previous was too short (~{} chars).\n\nOutline context:\n{}\n\nJust the story text, ~400 words.", ch_result.output.len(), &outline_text),
                0.7, 650, &format!("chapter {i} retry"),
            ).unwrap_or_else(|_| ch_result.output.clone());
            chapter_texts[i - 1] = corrected.clone();

            // Overwrite in workspace
            let fname = format!("03-CHAPTER_{i}.md");
            let p = ws.resolve(&fname).ok();
            if let Some(path) = p {
                std::fs::write(&path, &corrected).ok();
            }
        }
    }

    // ── Phase 4: Synopsis ──────────────────────────────────────────
    println!("📋 Synopsis...");
    let all_chapters = chapter_texts.iter()
        .enumerate()
        .map(|(i, t)| format!("## Chapter {}\n{}", i + 1, t))
        .collect::<Vec<_>>()
        .join("\n\n");
    let syn_plan = Plan {
        tasks: vec![Task {
            r#type: "write".into(),
            domain: "synopsis".into(),
            spec: serde_json::json!({"chapters": all_chapters}),
        }],
    };
    let _syn_result = agent.dispatch_single(&backend, &syn_plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("synopsis failed: {e}"))?;

    // ── Phase 5: Publish ───────────────────────────────────────────
    println!("📦 Publishing...");
    let pub_plan = Plan {
        tasks: vec![Task {
            r#type: "publish".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({}),
        }],
    };
    let pub_result = agent.dispatch_single(&backend, &pub_plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("publish failed: {e}"))?;

    // ── Commit ──────────────────────────────────────────────────────
    let outcome = agent.commit(pub_plan.clone(), vec![
        outline_result, wiki_result, pub_result,
    ], &ws)?;

    println!("\n✅ Done!\n");
    let mut files: Vec<_> = outcome.workspace_files.iter().collect();
    files.sort_by_key(|(k, _)| (*k).clone());
    for (fname, content) in files {
        println!("  📄 {} ({} bytes)\n     {}",
            fname, content.len(),
            content.lines().take(3).collect::<Vec<_>>().join("\n     "),
        );
    }

    println!("\n📂 Workspace path: {workspace_path}");
    println!("   Full story:    {}/06-STORY.md", workspace_path);

    Ok(())
}
