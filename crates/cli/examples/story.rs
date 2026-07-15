//! Story generation using the MechanisticAgent with grammar-constrained output.
//!
//! Pipeline: outline → wiki → chapter (×3) → validate → synopsis → publish.
//! Each handler uses grammar constraints to prevent think-tag contamination.
//!
//! Workspace artifacts produced:
//!   01-OUTLINE.md       — title, genre, tone, chapter summaries
//!   02-WIKI.md          — character bios, setting lore
//!   03-CHAPTER_{1,2,3}.md — chapter prose
//!   04-VALIDATION.md    — quality check results
//!   05-SYNOPSIS.md      — one-paragraph summary
//!   06-STORY.md         — complete published story
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

// ── Grammar Constraints ─────────────────────────────────────────────────────

/// Outline grammar: structured JSON with title, genre, tone, chapters.
const OUTLINE_GRAMMAR: &str = r#"
root  ::= "{" space "\"title\"" space ":" space string space "," space "\"genre\"" space ":" space string space "," space "\"tone\"" space ":" space string space "," space "\"chapters\"" space ":" space "[" space chapter ( "," space chapter )* "]" space "}"
chapter ::= "{" space "\"number\"" space ":" space number space "," space "\"title\"" space ":" space string space "," space "\"summary\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
number ::= [0-9]+
space ::= " "?
"#;

/// Wiki grammar: structured JSON with characters and setting.
const WIKI_GRAMMAR: &str = r#"
root  ::= "{" space "\"characters\"" space ":" space "[" space character ( "," space character )* "]" space "," space "\"setting\"" space ":" space string space "}"
character ::= "{" space "\"name\"" space ":" space string space "," space "\"description\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

/// Chapter grammar: structured JSON with title and content.
const CHAPTER_GRAMMAR: &str = r#"
root  ::= "{" space "\"title\"" space ":" space string space "," space "\"content\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

/// Validation grammar: structured JSON with quality, issues, suggestion.
const VALIDATION_GRAMMAR: &str = r#"
root  ::= "{" space "\"quality\"" space ":" space quality space "," space "\"issues\"" space ":" space string space "," space "\"suggestion\"" space ":" space string space "}"
quality ::= "\"pass\"" | "\"fail\"" | "\"needs-work\""
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

/// Synopsis grammar: structured JSON with summary text.
const SYNOPSIS_GRAMMAR: &str = r#"
root  ::= "{" space "\"summary\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

// ── Grammar-constrained completion ──────────────────────────────────────────

/// Make a grammar-constrained model call. No think-tag cleanup needed.
fn constrained_complete(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    grammar: &str,
    temperature: f32,
    max_tokens: usize,
) -> Result<String, String> {
    let resp = futures::executor::block_on(backend.complete(CompletionRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        grammar: Some(grammar.to_string()),
        temperature,
        max_tokens,
        ..Default::default()
    }))
    .map_err(|e| format!("model error: {e}"))?;

    Ok(resp.text)
}

// ── JSON helpers ────────────────────────────────────────────────────────────

/// Parse JSON string value, handling escape sequences.
fn parse_json_string(json: &str, key: &str) -> String {
    // Simple JSON string extraction (handles basic cases)
    let search = format!("\"{}\"", key);
    if let Some(pos) = json.find(&search) {
        let rest = &json[pos + search.len()..];
        // Find the colon and opening quote
        if let Some(colon_pos) = rest.find(':') {
            let after_colon = &rest[colon_pos + 1..];
            if let Some(quote_start) = after_colon.find('"') {
                let value_start = &after_colon[quote_start + 1..];
                // Find closing quote (handle escaped quotes)
                let mut result = String::new();
                let mut chars = value_start.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(&next) = chars.peek() {
                            match next {
                                'n' => { result.push('\n'); chars.next(); }
                                't' => { result.push('\t'); chars.next(); }
                                '"' => { result.push('"'); chars.next(); }
                                '\\' => { result.push('\\'); chars.next(); }
                                _ => { result.push(c); }
                            }
                        }
                    } else if c == '"' {
                        break;
                    } else {
                        result.push(c);
                    }
                }
                return result;
            }
        }
    }
    String::new()
}

// ── Handlers ────────────────────────────────────────────────────────────────

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
        
        let json = constrained_complete(
            backend,
            "You are a story outliner. Output valid JSON only.",
            &format!(
                "Outline a short story with 3 chapters based on this premise:\n{premise}\n\n\
                 Output JSON with: title, genre, tone, chapters (array of 3 objects with number, title, summary)",
            ),
            OUTLINE_GRAMMAR,
            0.6,
            300,
        ).unwrap_or_else(|_| {
            serde_json::json!({
                "title": "Untitled",
                "genre": "Unknown",
                "tone": "Unknown",
                "chapters": [
                    {"number": 1, "title": "Chapter 1", "summary": "Error generating outline"},
                    {"number": 2, "title": "Chapter 2", "summary": ""},
                    {"number": 3, "title": "Chapter 3", "summary": ""}
                ]
            }).to_string()
        });

        // Convert JSON to markdown
        let title = parse_json_string(&json, "title");
        let genre = parse_json_string(&json, "genre");
        let tone = parse_json_string(&json, "tone");
        
        let mut md = format!("Title: {title}\nGenre: {genre}\nTone: {tone}\n\n");
        
        // Parse chapters array
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json) {
            if let Some(chapters) = parsed.get("chapters").and_then(|c| c.as_array()) {
                for ch in chapters {
                    let num = ch.get("number").and_then(|n| n.as_u64()).unwrap_or(0);
                    let ch_title = ch.get("title").and_then(|t| t.as_str()).unwrap_or("");
                    let summary = ch.get("summary").and_then(|s| s.as_str()).unwrap_or("");
                    md.push_str(&format!("Chapter {num}: {ch_title}\n{summary}\n\n"));
                }
            }
        }

        let path = ws.resolve("01-OUTLINE.md").unwrap();
        std::fs::write(&path, &md).ok();
        
        HandlerResult {
            task: task.clone(),
            output: md,
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

        let json = constrained_complete(
            backend,
            "You are a worldbuilding assistant. Output valid JSON only.",
            &format!(
                "Based on this premise and outline, create character bios and setting lore:\n\n\
                 Premise: {premise}\nOutline: {outline}\n\n\
                 Output JSON with: characters (array of objects with name, description), setting (string)",
            ),
            WIKI_GRAMMAR,
            0.7,
            400,
        ).unwrap_or_else(|_| {
            serde_json::json!({
                "characters": [{"name": "Unknown", "description": "Error generating wiki"}],
                "setting": "Unknown"
            }).to_string()
        });

        // Convert JSON to markdown
        let mut md = String::new();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json) {
            md.push_str("Characters:\n");
            if let Some(chars) = parsed.get("characters").and_then(|c| c.as_array()) {
                for ch in chars {
                    let name = ch.get("name").and_then(|n| n.as_str()).unwrap_or("Unknown");
                    let desc = ch.get("description").and_then(|d| d.as_str()).unwrap_or("");
                    md.push_str(&format!("  - {name}: {desc}\n"));
                }
            }
            md.push('\n');
            let setting = parsed.get("setting").and_then(|s| s.as_str()).unwrap_or("Unknown");
            md.push_str(&format!("Setting:\n  - {setting}\n"));
        }

        let path = ws.resolve("02-WIKI.md").unwrap();
        std::fs::write(&path, &md).ok();
        
        HandlerResult {
            task: task.clone(),
            output: md,
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
                 Outline context:\n{outline}\n\n\
                 Output JSON with: title (string), content (string with the chapter prose)",
            )
        } else {
            format!(
                "Write {chapter_label}. Continue from where the previous chapter left off. \
                 Advance the plot. ~400 words.\n\n\
                 Previous chapter recap:\n{previous}\n\n\
                 Outline context:\n{outline}\n\n\
                 Output JSON with: title (string), content (string with the chapter prose)",
            )
        };

        let json = constrained_complete(
            backend,
            "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only.",
            &directive,
            CHAPTER_GRAMMAR,
            0.8,
            600,
        ).unwrap_or_else(|e| {
            serde_json::json!({
                "title": chapter_label,
                "content": format!("Error writing chapter: {e}")
            }).to_string()
        });

        // Convert JSON to markdown
        let title = parse_json_string(&json, "title");
        let content = parse_json_string(&json, "content");
        let md = format!("# {title}\n\n{content}");

        let filename = format!("03-CHAPTER_{}.md", chapter_num);
        let path = ws.resolve(&filename).unwrap();
        std::fs::write(&path, &md).ok();
        
        HandlerResult {
            task: task.clone(),
            output: md,
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

        let json = constrained_complete(
            backend,
            "You are a quality reviewer. Be strict. Output valid JSON only.",
            &format!(
                "Review this chapter and check for:\n\
                 1. Does it read like a coherent story (not meta-commentary)?\n\
                 2. Is the prose engaging?\n\n\
                 Chapter:\n{chapter_text}\n\n\
                 Output JSON with: quality (\"pass\" | \"fail\" | \"needs-work\"), issues (string), suggestion (string)",
            ),
            VALIDATION_GRAMMAR,
            0.3,
            200,
        ).unwrap_or_else(|_| {
            serde_json::json!({
                "quality": "fail",
                "issues": "Model error during validation",
                "suggestion": "Retry chapter generation"
            }).to_string()
        });

        // Convert JSON to markdown
        let mut md = String::new();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json) {
            let quality = parsed.get("quality").and_then(|q| q.as_str()).unwrap_or("fail");
            let issues = parsed.get("issues").and_then(|i| i.as_str()).unwrap_or("");
            let suggestion = parsed.get("suggestion").and_then(|s| s.as_str()).unwrap_or("");
            md.push_str(&format!("Quality: {quality}\nIssues: {issues}\nSuggestion: {suggestion}\n"));
        }

        // Append to VALIDATION.md
        let path = ws.resolve("04-VALIDATION.md").unwrap();
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let entry = format!("\n## Chapter {chapter_num}\n{md}");
        std::fs::write(&path, existing + &entry).ok();

        HandlerResult {
            task: task.clone(),
            output: md,
            files: HashMap::new(),
        }
    }));

    // ── write/synopsis ──────────────────────────────────────────────
    agent.register("write", "synopsis", Box::new(|task, backend, ws| {
        let chapters = task.spec.get("chapters")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        let json = constrained_complete(
            backend,
            "You are a literary summarizer. Output valid JSON only.",
            &format!(
                "Write a one-paragraph synopsis of the complete story based on these chapters:\n\n\
                 {chapters}\n\n\
                 Output JSON with: summary (string, one paragraph, ~100 words)",
            ),
            SYNOPSIS_GRAMMAR,
            0.5,
            200,
        ).unwrap_or_else(|e| {
            serde_json::json!({
                "summary": format!("Error writing synopsis: {e}")
            }).to_string()
        });

        // Convert JSON to markdown
        let summary = parse_json_string(&json, "summary");
        let md = format!("Synopsis:\n\n{summary}");

        let path = ws.resolve("05-SYNOPSIS.md").unwrap();
        std::fs::write(&path, &md).ok();
        
        HandlerResult {
            task: task.clone(),
            output: md,
            files: HashMap::new(),
        }
    }));

    // ── publish/chapter ────────────────────────────────────────────
    agent.register("publish", "chapter", Box::new(|_task, _backend, ws| {
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

    // Build execution plan
    let plan = Plan {
        tasks: vec![
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

        // Self-correction loop
        let val_path = ws.root().join("04-VALIDATION.md");
        if let Ok(val_content) = std::fs::read_to_string(&val_path) {
            if val_content.contains("Quality: fail") || val_content.contains("needs-work") {
                println!("⚠️  {} needs revision — retrying...", &chapter_label);
                
                // Retry with grammar constraint
                let retry_task = Task {
                    r#type: "write".into(),
                    domain: "chapter".into(),
                    spec: serde_json::json!({
                        "number": i,
                        "label": chapter_label,
                        "outline": outline_text,
                        "previous": previous,
                        "retry": true,
                    }),
                };
                let retry_result = agent.dispatch_single(&backend, &retry_task, &ws)
                    .unwrap_or(ch_result);

                let filename = format!("03-CHAPTER_{}.md", i);
                let path = ws.resolve(&filename).unwrap();
                std::fs::write(&path, &retry_result.output).ok();
                chapter_texts[i - 1] = retry_result.output;
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

fn sanitize_dirname(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
}

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

fn extract_title(outline: &str) -> String {
    for line in outline.lines() {
        if line.starts_with("Title:") {
            return line.trim_start_matches("Title:").trim().to_string();
        }
    }
    "Untitled Story".to_string()
}
