//! Story generation using the MechanisticAgent with structured (schema→GBNF) output.
//!
//! Pipeline: outline → wiki → chapter (×3) → validate → synopsis → publish.
//! Every model call is constrained by a GBNF grammar auto-generated from a
//! JSON Schema — the model never outputs free-form text. Output is parsed
//! directly into typed Rust structs via serde, then rendered to markdown.
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

use roco_agent::mechanistic::{HandlerResult, MechanisticAgent, Plan, RepairConfig, Task};
use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::{schema_to_gbnf, Schema};
use roco_inference::RwkvBackend;
use roco_tools::Tool;
use roco_tools::{ReadTool, WriteTool};
use roco_workspace::{Workspace, WorkspaceKind};
use serde::Deserialize;
use serde_json::json;

// ═════════════════════════════════════════════════════════════════════════════
// Typed output structs — one per pipeline stage
// ═════════════════════════════════════════════════════════════════════════════

/// Model output for the outline stage.
#[derive(Debug, Deserialize)]
struct Outline {
    title: String,
    genre: String,
    tone: String,
    chapters: Vec<ChapterInfo>,
}

#[derive(Debug, Deserialize)]
struct ChapterInfo {
    number: u64,
    title: String,
    summary: String,
}

impl Outline {
    fn schema() -> Schema {
        Schema::object()
            .prop("title", Schema::string())
            .prop("genre", Schema::string())
            .prop("tone", Schema::string())
            .prop(
                "chapters",
                Schema::array(
                    Schema::object()
                        .prop("number", Schema::integer())
                        .prop("title", Schema::string())
                        .prop("summary", Schema::string())
                        .build(),
                ),
            )
            .build()
    }

    fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("Outline schema is valid")
    }
}

/// Model output for the wiki/worldbuilding stage.
#[derive(Debug, Deserialize)]
struct Wiki {
    characters: Vec<Character>,
    setting: String,
}

#[derive(Debug, Deserialize)]
struct Character {
    name: String,
    description: String,
}

impl Wiki {
    fn schema() -> Schema {
        Schema::object()
            .prop(
                "characters",
                Schema::array(
                    Schema::object()
                        .prop("name", Schema::string())
                        .prop("description", Schema::string())
                        .build(),
                ),
            )
            .prop("setting", Schema::string())
            .build()
    }

    fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("Wiki schema is valid")
    }
}

/// Model output for a single chapter.
#[derive(Debug, Deserialize)]
struct Chapter {
    title: String,
    content: String,
}

impl Chapter {
    fn schema() -> Schema {
        Schema::object()
            .prop("title", Schema::string())
            .prop("content", Schema::string())
            .build()
    }

    fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("Chapter schema is valid")
    }
}

/// Model output for validation.
#[derive(Debug, Deserialize)]
struct Validation {
    quality: String, // "pass" | "fail" | "needs-work"
    issues: String,
    suggestion: String,
}

impl Validation {
    fn schema() -> Schema {
        Schema::object()
            .prop(
                "quality",
                Schema::enum_values(vec![
                    serde_json::json!("pass"),
                    serde_json::json!("fail"),
                    serde_json::json!("needs-work"),
                ]),
            )
            .prop("issues", Schema::string())
            .prop("suggestion", Schema::string())
            .build()
    }

    fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("Validation schema is valid")
    }
}

/// Model output for the synopsis.
#[derive(Debug, Deserialize)]
struct Synopsis {
    summary: String,
}

impl Synopsis {
    fn schema() -> Schema {
        Schema::object().prop("summary", Schema::string()).build()
    }

    fn grammar() -> String {
        schema_to_gbnf("root", Self::schema().to_json()).expect("Synopsis schema is valid")
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Structured completion — grammar-constrained model call with typed output
// ═════════════════════════════════════════════════════════════════════════════

/// Call the model with a grammar constraint derived from a JSON Schema,
/// then deserialize the guaranteed-valid JSON output into the target type.
fn structured_complete<T>(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    grammar: &str,
    temperature: f32,
    max_tokens: usize,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let text = futures::executor::block_on(backend.complete(CompletionRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        grammar: Some(grammar.to_string()),
        temperature,
        max_tokens,
        ..Default::default()
    }))
    .map_err(|e| format!("model error: {e}"))?
    .text;

    serde_json::from_str::<T>(&text)
        .map_err(|e| format!("parse error (grammar constraint violated?): {e}\nraw: {text}"))
}

// ═════════════════════════════════════════════════════════════════════════════
// Handlers
// ═════════════════════════════════════════════════════════════════════════════

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let prompt = std::env::args().nth(1).unwrap_or_else(|| {
        "Write a short story about a lighthouse keeper who discovers a message in a bottle."
            .to_string()
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

    agent.add_route(
        "storyTeller",
        vec![
            ("compose", "outline"),
            ("compose", "wiki"),
            ("write", "chapter"),
            ("write", "synopsis"),
            ("validate", "chapter"),
            ("publish", "chapter"),
        ],
    );

    // ── compose/outline ──────────────────────────────────────────────
    agent.register(
        "compose",
        "outline",
        Box::new(|task, backend, ws| {
            let premise = task
                .spec
                .get("premise")
                .and_then(|v| v.as_str())
                .unwrap_or("a short story");

            let outline: Outline = structured_complete(
                backend,
                "You are a story outliner. Output valid JSON only.",
                &format!(
                    "Outline a short story with 3 chapters based on this premise:\n{premise}\n\n\
                 Output JSON matching the schema: title, genre, tone, chapters \
                 (array of 3 objects with number, title, summary)",
                ),
                &Outline::grammar(),
                0.6,
                300,
            )
            .unwrap_or_else(|e| Outline {
                title: "Untitled".into(),
                genre: "Unknown".into(),
                tone: "Unknown".into(),
                chapters: (1..=3)
                    .map(|i| ChapterInfo {
                        number: i,
                        title: format!("Chapter {i}"),
                        summary: format!("Error generating outline: {e}"),
                    })
                    .collect(),
            });

            // Render to markdown
            let mut md = format!(
                "Title: {}\nGenre: {}\nTone: {}\n\n",
                outline.title, outline.genre, outline.tone
            );
            for ch in &outline.chapters {
                md.push_str(&format!(
                    "Chapter {}: {}\n{}\n\n",
                    ch.number, ch.title, ch.summary
                ));
            }

            let path = ws.resolve("01-OUTLINE.md").unwrap();
            let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

            HandlerResult {
                task: task.clone(),
                output: md,
                files: HashMap::new(),
                pass: true,
            }
        }),
    );

    // ── compose/wiki ────────────────────────────────────────────────
    agent.register("compose", "wiki", Box::new(|task, backend, ws| {
        let premise = task.spec.get("premise")
            .and_then(|v| v.as_str())
            .unwrap_or("a short story");
        let outline = task.spec.get("outline")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let wiki: Wiki = structured_complete(
            backend,
            "You are a worldbuilding assistant. Output valid JSON only.",
            &format!(
                "Based on this premise and outline, create character bios and setting lore:\n\n\
                 Premise: {premise}\nOutline: {outline}\n\n\
                 Output JSON matching the schema: characters (array of objects with name, description), \
                 setting (string)",
            ),
            &Wiki::grammar(),
            0.7,
            400,
        ).unwrap_or_else(|e| Wiki {
            characters: vec![Character {
                name: "Unknown".into(),
                description: format!("Error generating wiki: {e}"),
            }],
            setting: "Unknown".into(),
        });

        // Render to markdown
        let mut md = String::from("Characters:\n");
        for ch in &wiki.characters {
            md.push_str(&format!("  - {}: {}\n", ch.name, ch.description));
        }
        md.push('\n');
        md.push_str(&format!("Setting:\n  - {}\n", wiki.setting));

        let path = ws.resolve("02-WIKI.md").unwrap();
        let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

        HandlerResult {
            task: task.clone(),
            output: md,
            files: HashMap::new(),
            pass: true,
        }
    }));

    // ── write/chapter ────────────────────────────────────────────────
    agent.register(
        "write",
        "chapter",
        Box::new(|task, backend, ws| {
            let chapter_num: usize = task
                .spec
                .get("number")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize;
            let chapter_label = task
                .spec
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("Chapter 1");
            let outline = task
                .spec
                .get("outline")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let previous = task
                .spec
                .get("previous")
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

            let chapter: Chapter = structured_complete(
                backend,
                "You are a fiction writer. Write vivid, engaging prose. Output valid JSON only.",
                &directive,
                &Chapter::grammar(),
                0.8,
                600,
            )
            .unwrap_or_else(|e| Chapter {
                title: chapter_label.into(),
                content: format!("Error writing chapter: {e}"),
            });

            let md = format!("# {}\n\n{}", chapter.title, chapter.content);

            let filename = format!("03-CHAPTER_{}.md", chapter_num);
            let path = ws.resolve(&filename).unwrap();
            let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

            HandlerResult {
                task: task.clone(),
                output: md,
                files: HashMap::new(),
                pass: true,
            }
        }),
    );

    // ── validate/chapter ────────────────────────────────────────────
    agent.register("validate", "chapter", Box::new(|task, backend, ws| {
        let chapter_text = task.spec.get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let chapter_num = task.spec.get("number")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let entry = if chapter_text.trim().is_empty() {
            format!("\n## Chapter {chapter_num}\n[validation skipped — chapter is empty]\n")
        } else {
            structured_complete::<Validation>(
                backend,
                "You are a quality reviewer. Be strict. Output valid JSON only.",
                &format!(
                    "Review this chapter and check for:\n\
                     1. Does it read like a coherent story (not meta-commentary)?\n\
                     2. Is the prose engaging?\n\n\
                     Chapter:\n{chapter_text}\n\n\
                     Output JSON matching the schema: quality (\"pass\" | \"fail\" | \"needs-work\"), \
                     issues (string), suggestion (string)",
                ),
                &Validation::grammar(),
                0.3,
                200,
            ).map(|v: Validation| {
                format!("\n## Chapter {chapter_num}\nQuality: {}\nIssues: {}\nSuggestion: {}\n",
                        v.quality, v.issues, v.suggestion)
            }).unwrap_or_else(|e| {
                format!("\n## Chapter {chapter_num}\nQuality: fail\nIssues: Model error: {e}\nSuggestion: Retry\n")
            })
        };

        // Append to VALIDATION.md
        let path = ws.resolve("04-VALIDATION.md").unwrap();
        let existing = ReadTool
            .call(json!({"path": path.to_string_lossy()}))
            .ok()
            .and_then(|v| v.get("content").and_then(|c| c.as_str().map(String::from)))
            .unwrap_or_default();
        let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": existing + &entry}));

        HandlerResult {
            task: task.clone(),
            output: entry,
            files: HashMap::new(),
            pass: true,
        }
    }));

    // ── write/synopsis ──────────────────────────────────────────────
    agent.register(
        "write",
        "synopsis",
        Box::new(|task, backend, ws| {
            let chapters = task
                .spec
                .get("chapters")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let synopsis: Synopsis = structured_complete(
                backend,
                "You are a literary summarizer. Output valid JSON only.",
                &format!(
                "Write a one-paragraph synopsis of the complete story based on these chapters:\n\n\
                 {chapters}\n\n\
                 Output JSON matching the schema: summary (string, one paragraph, ~100 words)",
            ),
                &Synopsis::grammar(),
                0.5,
                200,
            )
            .unwrap_or_else(|e| Synopsis {
                summary: format!("Error writing synopsis: {e}"),
            });

            let md = format!("Synopsis:\n\n{}", synopsis.summary);

            let path = ws.resolve("05-SYNOPSIS.md").unwrap();
            let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

            HandlerResult {
                task: task.clone(),
                output: md,
                files: HashMap::new(),
                pass: true,
            }
        }),
    );

    // ── publish/chapter ────────────────────────────────────────────
    agent.register("publish", "chapter", Box::new(|_task, _backend, ws| {
        let read_file = |name: &str| -> String {
            ReadTool
                .call(json!({"path": ws.root().join(name).to_string_lossy()}))
                .ok()
                .and_then(|v| v.get("content").and_then(|c| c.as_str().map(String::from)))
                .unwrap_or_default()
        };
        let outline = read_file("01-OUTLINE.md");
        let wiki = read_file("02-WIKI.md");
        let mut story = format!("# {}\n\n", extract_title(&outline));

        if !wiki.is_empty() {
            story.push_str("## Characters & Setting\n\n");
            story.push_str(&wiki);
            story.push_str("\n\n---\n\n");
        }

        for i in 1..=3 {
            let ch = ReadTool
                .call(json!({"path": ws.root().join(format!("03-CHAPTER_{i}.md")).to_string_lossy()}))
                .ok()
                .and_then(|v| v.get("content").and_then(|c| c.as_str().map(String::from)))
                .unwrap_or_default();
            if !ch.is_empty() {
                story.push_str(&ch);
                story.push_str("\n\n---\n\n");
            }
        }

        let synopsis = read_file("05-SYNOPSIS.md");
        if !synopsis.is_empty() {
            story.push_str("## Synopsis\n\n");
            story.push_str(&synopsis);
            story.push_str("\n");
        }

        let path = ws.resolve("06-STORY.md").unwrap();
        let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &story}));

        HandlerResult {
            task: Task {
                r#type: "publish".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({"status": "published"}),
            },
            output: format!("published {} bytes", story.len()),
            files: HashMap::new(),
            pass: true,
        }
    }));

    // Build execution plan
    let plan = Plan {
        tasks: vec![Task {
            r#type: "compose".into(),
            domain: "outline".into(),
            spec: serde_json::json!({"premise": prompt}),
        }],
    };

    let ws = create_workspace(&prompt)?;
    let workspace_path = ws.root().to_string_lossy().to_string();

    println!("\nWorkspace: {workspace_path}\n");
    println!("Pipeline: outline → wiki → chapter×3 → validate → synopsis → publish\n");

    // Phase 1: outline
    println!("📝 Outline...");
    let outline_result = agent
        .dispatch_single(&backend, &plan.tasks[0], &ws)
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
    let wiki_result = agent
        .dispatch_single(&backend, &wiki_plan.tasks[0], &ws)
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
        let ch_result = agent
            .dispatch_single(&backend, &ch_task, &ws)
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
        let _val_result = agent
            .dispatch_single(&backend, &val_task, &ws)
            .map_err(|e| anyhow::anyhow!("validation {i} failed: {e}"))?;

        // Self-correction loop
        let val_path = ws.root().join("04-VALIDATION.md");
        if let Some(val_content) = ReadTool
            .call(json!({"path": val_path.to_string_lossy()}))
            .ok()
            .and_then(|v| v.get("content").and_then(|c| c.as_str().map(String::from)))
        {
            let chapter_header = format!("## Chapter {i}");
            let needs_revision = if let Some(start_idx) = val_content.find(&chapter_header) {
                let segment = &val_content[start_idx..];
                let next_chapter_header = format!("## Chapter {}", i + 1);
                let segment = if let Some(end_idx) = segment.find(&next_chapter_header) {
                    &segment[..end_idx]
                } else {
                    segment
                };
                segment.contains("Quality: fail")
                    || segment.contains("Quality: needs-work")
                    || segment.contains("needs-work")
            } else {
                false
            };

            if needs_revision {
                println!("⚠️  {} needs revision — retrying...", &chapter_label);

                // Retry — re-use the structured grammar constraint
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
                let retry_result = agent
                    .dispatch_single(&backend, &retry_task, &ws)
                    .unwrap_or(ch_result);

                let filename = format!("03-CHAPTER_{}.md", i);
                let path = ws.resolve(&filename).unwrap();
                let _ = WriteTool
                    .call(json!({"path": path.to_string_lossy(), "content": &retry_result.output}));
                chapter_texts[i - 1] = retry_result.output;
            }
        }
    }

    // Phase 4: synopsis
    println!("📋 Synopsis...");
    let all_chapters = chapter_texts
        .iter()
        .enumerate()
        .map(|(i, t)| format!("## Chapter {}\n{}", i + 1, t))
        .collect::<Vec<_>>()
        .join("\n\n");
    let synopsis_task = Task {
        r#type: "write".into(),
        domain: "synopsis".into(),
        spec: serde_json::json!({"chapters": all_chapters}),
    };
    let _synopsis_result = agent
        .dispatch_single(&backend, &synopsis_task, &ws)
        .map_err(|e| anyhow::anyhow!("synopsis failed: {e}"))?;

    // Phase 5: publish
    println!("📦 Publishing...");
    let publish_task = Task {
        r#type: "publish".into(),
        domain: "chapter".into(),
        spec: serde_json::json!({}),
    };
    let publish_result = agent
        .dispatch_single(&backend, &publish_task, &ws)
        .map_err(|e| anyhow::anyhow!("publish failed: {e}"))?;

    let outcome = agent.commit(
        plan.clone(),
        vec![outline_result, wiki_result, publish_result],
        &ws,
    )?;

    println!(
        "✅ Done! {} files in workspace:\n",
        outcome.workspace_files.len()
    );
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
