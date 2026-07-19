//! Story Pilot — MechanisticAgent-driven story generation.
//!
//! Demonstrates the MechanisticAgent pattern:
//! - Register routes and handlers for a "storyTeller" workflow
//! - Build a structured Plan in code (reliable)
//! - Dispatch tasks → handlers write to sandboxed workspace
//! - Commit → snapshot all workspace files
//!
//! All model calls use grammar constraints to prevent think-tag contamination.
//!
//! Output lands in `.roco/workspaces/story_<prompt>_<ts>/`.
//!
//! Usage:
//!   cargo run --release --example story_pilot -p roco-cli \
//!     "Make me a xianxia story about a lone cultivator who levels up alone"

use std::collections::HashMap;
use std::time::SystemTime;

use roco_agent::mechanistic::{HandlerResult, MechanisticAgent, Plan, RepairConfig, Task};
use roco_engine::{CompletionRequest, ModelBackend};
use roco_inference::RwkvBackend;
use roco_workspace::{Workspace, WorkspaceKind};
use tracing::{error, info};

// ── Grammar Constraints ──────────────────────────────────────────────

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

/// Synopsis grammar: structured JSON with summary text.
const SYNOPSIS_GRAMMAR: &str = r#"
root  ::= "{" space "\"summary\"" space ":" space string space "}"
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

// ── Grammar-constrained completion ──────────────────────────────────

/// Make a grammar-constrained model call. No think-tag cleanup needed.
fn constrained_complete(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    grammar: &str,
    temperature: f32,
    max_tokens: usize,
) -> anyhow::Result<String> {
    let resp = futures::executor::block_on(backend.complete(CompletionRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        grammar: Some(grammar.to_string()),
        temperature,
        max_tokens,
        ..Default::default()
    }))
    .map_err(|e| anyhow::anyhow!("model error: {e}"))?;

    Ok(resp.text)
}

// ── JSON helpers ────────────────────────────────────────────────────

/// Parse JSON string value, handling escape sequences.
fn parse_json_string(json: &str, key: &str) -> String {
    let search = format!("\"{}\"", key);
    if let Some(pos) = json.find(&search) {
        let rest = &json[pos + search.len()..];
        if let Some(colon_pos) = rest.find(':') {
            let after_colon = &rest[colon_pos + 1..];
            if let Some(quote_start) = after_colon.find('"') {
                let value_start = &after_colon[quote_start + 1..];
                let mut result = String::new();
                let mut chars = value_start.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        if let Some(&next) = chars.peek() {
                            match next {
                                'n' => {
                                    result.push('\n');
                                    chars.next();
                                }
                                't' => {
                                    result.push('\t');
                                    chars.next();
                                }
                                '"' => {
                                    result.push('"');
                                    chars.next();
                                }
                                '\\' => {
                                    result.push('\\');
                                    chars.next();
                                }
                                _ => {
                                    result.push(c);
                                }
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

// ── Handlers ────────────────────────────────────────────────────────

fn handle_outline(task: &Task, backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let premise = task
        .spec
        .get("premise")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    info!(premise = %premise, "generate outline");

    let json = match constrained_complete(
        backend,
        "You are a story outliner. Output valid JSON only.",
        &format!(
            "Outline a 3-chapter story from:\n{premise}\n\n\
             Output JSON with: title, genre, tone, chapters (array of 3 objects with number, title, summary)",
        ),
        OUTLINE_GRAMMAR,
        0.6,
        350,
    ) {
        Ok(j) => j,
        Err(e) => { error!(%e); return error_result(task, "outline"); }
    };

    // Convert JSON to markdown
    let title = parse_json_string(&json, "title");
    let genre = parse_json_string(&json, "genre");
    let tone = parse_json_string(&json, "tone");

    let mut md = format!("Title: {title}\nGenre: {genre}\nTone: {tone}\n\n");

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

    let path = ws.resolve("01-OUTLINE.md");
    if let Ok(p) = &path {
        std::fs::write(p, &md).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: md.clone(),
        files: HashMap::from([("01-OUTLINE.md".into(), md)]),
        pass: true,
    }
}

fn handle_wiki(task: &Task, backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let premise = task
        .spec
        .get("premise")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let outline = task
        .spec
        .get("outline")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    info!("generate wiki");

    let json = match constrained_complete(
        backend,
        "You are a worldbuilding assistant. Output valid JSON only.",
        &format!(
            "Create characters and setting lore for this story:\nPremise: {premise}\nOutline: {outline}\n\n\
             Output JSON with: characters (array of objects with name, description), setting (string)",
        ),
        WIKI_GRAMMAR,
        0.7,
        450,
    ) {
        Ok(j) => j,
        Err(e) => { error!(%e); return error_result(task, "wiki"); }
    };

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
        let setting = parsed
            .get("setting")
            .and_then(|s| s.as_str())
            .unwrap_or("Unknown");
        md.push_str(&format!("Setting:\n  - {setting}\n"));
    }

    let path = ws.resolve("02-WIKI.md");
    if let Ok(p) = &path {
        std::fs::write(p, &md).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: md.clone(),
        files: HashMap::from([("02-WIKI.md".into(), md)]),
        pass: true,
    }
}

fn handle_chapter(task: &Task, backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let num = task
        .spec
        .get("number")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as usize;
    let outline = task
        .spec
        .get("outline")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let prev = task
        .spec
        .get("previous")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let directive = if num == 1 {
        format!(
            "Write Chapter 1. Introduce character and setting. ~400 words.\n\
             Outline:\n{outline}\n\n\
             Output JSON with: title (string), content (string with the chapter prose)",
        )
    } else {
        format!(
            "Write Chapter {num}. Continue from previous. Advance plot. ~400 words.\n\
             Previous ending:\n{prev}\n\n\
             Outline:\n{outline}\n\n\
             Output JSON with: title (string), content (string with the chapter prose)",
        )
    };

    let json = match constrained_complete(
        backend,
        "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only.",
        &directive,
        CHAPTER_GRAMMAR,
        0.8,
        650,
    ) {
        Ok(j) => j,
        Err(e) => {
            error!(%e);
            return error_result(task, &format!("chapter {num}"));
        }
    };

    // Convert JSON to markdown
    let title = parse_json_string(&json, "title");
    let content = parse_json_string(&json, "content");
    let md = format!("# {title}\n\n{content}");

    let fname = format!("03-CHAPTER_{num}.md");
    let path = ws.resolve(&fname);
    if let Ok(p) = &path {
        std::fs::write(p, &md).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: md.clone(),
        files: HashMap::from([(fname, md)]),
        pass: true,
    }
}

/// BNF grammar for validation output — structured JSON, no think-tag leaks.
const VALIDATE_GRAMMAR: &str = r#"
root  ::= "{" space "\"pass\"" space ":" space bool space "," space "\"reason\"" space ":" space string space "}"
bool  ::= "true" | "false"
string ::= "\"" ( [ -~] )* "\""
space ::= " "?
"#;

fn validate_wiki_quality(
    backend: &dyn ModelBackend,
    ws: &Workspace,
    wiki_text: &str,
    outline_text: &str,
) -> HandlerResult {
    let reason: String = serde_json::from_str::<serde_json::Value>(
        &match constrained_complete(
            backend,
            "You are a story quality inspector. Output valid JSON only. Do not generate story content.",
            &format!(
                "Evaluate this wiki/worldbuilding against the outline.\nOutline:\n{}\n\nWiki:\n{}\n\n\
                 Check: Does wiki contain named characters? Concrete setting details? Useful lore?\n\
                 Return only: {{\"pass\": true/false, \"reason\": \"short reason\"}}",
                outline_text.chars().take(60).collect::<String>(), wiki_text.chars().take(800).collect::<String>(),
            ),
            VALIDATE_GRAMMAR, 0.5, 80,
        ) {
            Ok(j) => j,
            Err(_) => format!("{{\"pass\": {}, \"reason\": \"parse fallback\"}}", wiki_text.len() > 50),
        }
    ).ok()
        .and_then(|v| v.get("reason").map(|r| r.to_string()))
        .or_else(|| Some("quality check failed".into()))
        .unwrap_or_default();

    let ch_len = wiki_text.len();
    let pass = serde_json::from_str::<serde_json::Value>(&reason)
        .ok()
        .as_ref()
        .and_then(|v| v.get("pass").and_then(|p| p.as_bool()))
        .unwrap_or(ch_len > 50);

    let note = if !pass {
        format!(
            "Wiki: FAIL — {} ({ch_len} chars)",
            reason.trim_matches('\"')
        )
    } else {
        format!("Wiki: OK ({ch_len} chars)")
    };

    let path = ws.resolve("04-VALIDATION.md");
    if let Ok(p) = path {
        let existing = std::fs::read_to_string(&p).unwrap_or_default();
        std::fs::write(&p, format!("{existing}\n## Wiki\n{note}\n")).ok();
    }

    HandlerResult {
        task: Task {
            r#type: "validate".into(),
            domain: "wiki".into(),
            spec: Default::default(),
        },
        output: note,
        files: HashMap::new(),
        pass,
    }
}

fn handle_validate(task: &Task, backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let num = task
        .spec
        .get("number")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let text = task.spec.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let outline = task
        .spec
        .get("outline")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    info!(chapter = num, "validate chapter");
    // Truncate long chapters for prompt size budget
    let prompt_text = if text.len() > 800 { &text[..800] } else { text };

    // Grammar-constrained quality check: length, plot markers, variety
    let ch_len = text.len();
    let json = match constrained_complete(
        backend,
        "You are a story quality inspector. Output valid JSON only. Do not generate story content.",
        &format!(
            "Evaluate Chapter {} of {}.\n\nContent:\n{}\n\n\
             Check: length >{} chars? Concrete nouns present? Plot advancement? Repetition?\n\
             Return only: {{\"pass\": true/false, \"reason\": \"short reason\"}}",
            num,
            outline.trim_end().chars().take(80).collect::<String>(),
            prompt_text,
            400,
        ),
        VALIDATE_GRAMMAR,
        0.5,
        80,
    ) {
        Ok(j) => j,
        Err(_) => {
            serde_json::json!({"pass": text.len() > 400, "reason": "parse fallback"}).to_string()
        }
    };

    let pass = serde_json::from_str::<serde_json::Value>(&json)
        .ok()
        .as_ref()
        .and_then(|v| v.get("pass").and_then(|p| p.as_bool()))
        .unwrap_or(text.len() > 400);

    let note = if !pass {
        let reason: String = serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .and_then(|v| v.get("reason").map(|r| r.to_string()))
            .or_else(|| Some("quality check failed".into()))
            .unwrap_or_default();
        format!("Chapter {num}: FAIL — {reason}")
    } else {
        format!("Chapter {num}: OK ({ch_len} chars)")
    };

    let path = ws.resolve("04-VALIDATION.md");
    if let Ok(p) = path {
        let existing = std::fs::read_to_string(&p).unwrap_or_default();
        std::fs::write(&p, format!("{existing}\n## Ch{num}\n{note}\n")).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: note,
        files: HashMap::new(),
        pass,
    }
}

fn handle_synopsis(task: &Task, backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    let chapters = task
        .spec
        .get("chapters")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    info!("generate synopsis");

    let json = match constrained_complete(
        backend,
        "You are a literary summarizer. Output valid JSON only.",
        &format!(
            "Write a one-paragraph synopsis of the complete story based on these chapters:\n\n\
             {chapters}\n\n\
             Output JSON with: summary (string, one paragraph, ~100 words)",
        ),
        SYNOPSIS_GRAMMAR,
        0.5,
        250,
    ) {
        Ok(j) => j,
        Err(e) => {
            error!(%e);
            return error_result(task, "synopsis");
        }
    };

    let summary = parse_json_string(&json, "summary");
    let md = format!("Synopsis:\n\n{summary}");

    let path = ws.resolve("05-SYNOPSIS.md");
    if let Ok(p) = &path {
        std::fs::write(p, &md).ok();
    }

    HandlerResult {
        task: task.clone(),
        output: md.clone(),
        files: HashMap::from([("05-SYNOPSIS.md".into(), md)]),
        pass: true,
    }
}

fn handle_publish(_task: &Task, _backend: &dyn ModelBackend, ws: &Workspace) -> HandlerResult {
    info!("assemble final story");

    let outline = std::fs::read_to_string(ws.root().join("01-OUTLINE.md")).unwrap_or_default();
    let wiki = std::fs::read_to_string(ws.root().join("02-WIKI.md")).unwrap_or_default();
    let synopsis = std::fs::read_to_string(ws.root().join("05-SYNOPSIS.md")).unwrap_or_default();

    let title = outline
        .lines()
        .find(|l| l.starts_with("Title:") || l.starts_with("# "))
        .map(|l| l.replace("Title:", "").replace("# ", "").trim().to_string())
        .unwrap_or_else(|| "Untitled Story".into());

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
        pass: true,
    }
}

// ── Error handling ──────────────────────────────────────────────────

fn error_result(task: &Task, label: &str) -> HandlerResult {
    let error_msg = format!("[{label} error]");
    HandlerResult {
        task: task.clone(),
        output: error_msg.clone(),
        files: HashMap::new(),
        pass: false,
    }
}

// ── Utilities ───────────────────────────────────────────────────────

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

fn create_workspace(prompt: &str) -> anyhow::Result<Workspace> {
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
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
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let premise = std::env::args().nth(1).unwrap_or_else(|| {
        "Write a story about a lone cultivator who discovers a hidden inheritance".into()
    });

    info!(%premise, "StoryPilot starting");
    println!("Loading model...\n");
    let backend = RwkvBackend::from_env()?;
    println!("Model ready.\n");

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

    agent.add_route(
        "storyTeller",
        vec![
            ("compose", "outline"),
            ("compose", "wiki"),
            ("write", "chapter"),
            ("validate", "chapter"),
            ("write", "synopsis"),
            ("publish", "chapter"),
        ],
    );

    agent.register("compose", "outline", Box::new(handle_outline));
    agent.register("compose", "wiki", Box::new(handle_wiki));
    agent.register("write", "chapter", Box::new(handle_chapter));
    agent.register("validate", "chapter", Box::new(handle_validate));
    agent.register("write", "synopsis", Box::new(handle_synopsis));
    agent.register("publish", "chapter", Box::new(handle_publish));

    let ws = create_workspace(&premise)?;
    let workspace_path = ws.root().to_string_lossy().to_string();

    println!("Pipeline: outline → wiki → chapter×3 → validate → synopsis → publish\n");

    // Phase 1: Outline
    println!("📝 Outline...");
    let outline_plan = Plan {
        tasks: vec![Task {
            r#type: "compose".into(),
            domain: "outline".into(),
            spec: serde_json::json!({"premise": premise}),
        }],
    };
    let outline_result = agent
        .dispatch_single(&backend, &outline_plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("outline failed: {e}"))?;
    let outline_text = outline_result.output.clone();

    // Phase 2: Wiki
    println!("📚 Worldbuilding...");
    let wiki_plan = Plan {
        tasks: vec![Task {
            r#type: "compose".into(),
            domain: "wiki".into(),
            spec: serde_json::json!({"premise": &premise, "outline": &outline_text}),
        }],
    };
    let mut wiki_result = agent
        .dispatch_single(&backend, &wiki_plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("wiki failed: {e}"))?;

    // Validate wiki quality before proceeding
    println!("   🔍 Validating wiki...");
    let wiki_val = validate_wiki_quality(&backend, &ws, &wiki_result.output, &outline_text);
    if !wiki_val.pass {
        println!(
            "   ⚠️  Wiki needs improvement ({}) — regenerating...",
            wiki_val.output.trim()
        );
        // Regenerate wiki with quality feedback
        let retry_task = Task {
            r#type: "compose".into(),
            domain: "wiki".into(),
            spec: serde_json::json!({"premise": &premise, "outline": &outline_text, "quality_note": &wiki_val.output}),
        };
        let retry_wiki = agent
            .dispatch_single(&backend, &retry_task, &ws)
            .unwrap_or(wiki_result.clone());
        let wiki_path = ws.resolve("02-WIKI.md").ok();
        if let Some(path) = wiki_path {
            std::fs::write(&path, &retry_wiki.output).ok();
        }
        wiki_result = retry_wiki;
    }

    // Phase 3: Chapters ×3
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
        let ch_result = agent
            .dispatch_single(&backend, &ch_task, &ws)
            .map_err(|e| anyhow::anyhow!("chapter {i} failed: {e}"))?;
        chapter_texts.push(ch_result.output.clone());

        // Validate
        println!("   🔍 Validating...");
        let val_task = Task {
            r#type: "validate".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({"number": i, "text": ch_result.output, "outline": &outline_text}),
        };
        let val_result = agent
            .dispatch_single(&backend, &val_task, &ws)
            .map_err(|e| anyhow::anyhow!("validation {i} failed: {e}"))?;

        // Self-correction: low-quality or too-short chapter → rewrite
        if !val_result.pass || ch_result.output.len() < 400 {
            let reason_txt = &val_result.output;
            println!("   ⚠️  Chapter {i} needs improvement ({reason_txt}) — retrying...");
            let retry_task = Task {
                r#type: "write".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({
                    "number": i,
                    "outline": &outline_text,
                    "previous": prev,
                    "retry": true,
                }),
            };
            let retry_result = agent
                .dispatch_single(&backend, &retry_task, &ws)
                .unwrap_or(ch_result);
            chapter_texts[i - 1] = retry_result.output.clone();

            let fname = format!("03-CHAPTER_{i}.md");
            let p = ws.resolve(&fname).ok();
            if let Some(path) = p {
                std::fs::write(&path, &retry_result.output).ok();
            }
        }
    }

    // Phase 4: Synopsis
    println!("📋 Synopsis...");
    let all_chapters = chapter_texts
        .iter()
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
    let _syn_result = agent
        .dispatch_single(&backend, &syn_plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("synopsis failed: {e}"))?;

    // Phase 5: Publish
    println!("📦 Publishing...");
    let pub_plan = Plan {
        tasks: vec![Task {
            r#type: "publish".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({}),
        }],
    };
    let pub_result = agent
        .dispatch_single(&backend, &pub_plan.tasks[0], &ws)
        .map_err(|e| anyhow::anyhow!("publish failed: {e}"))?;

    // Commit
    let outcome = agent.commit(
        pub_plan.clone(),
        vec![outline_result, wiki_result, pub_result],
        &ws,
    )?;

    println!("\n✅ Done!\n");
    let mut files: Vec<_> = outcome.workspace_files.iter().collect();
    files.sort_by_key(|(k, _)| (*k).clone());
    for (fname, content) in files {
        println!(
            "  📄 {} ({} bytes)\n     {}",
            fname,
            content.len(),
            content.lines().take(3).collect::<Vec<_>>().join("\n     "),
        );
    }

    println!("\n📂 Workspace path: {workspace_path}");
    println!("   Full story:    {}/06-STORY.md", workspace_path);

    Ok(())
}
