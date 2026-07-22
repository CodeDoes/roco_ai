//! Story subcommand: `roco story` — structured short story pipeline.
//!
//! Generates an outline → wiki → chapters (with validation & correction)
//! → synopsis → published story in a sandbox workspace + `.roco/stories/`.
//!
//! Writes to an AgentJournal so the user can `tail -f .roco/agent-journal.md`
//! and see what the agent is doing in real time.
//!
//! # Format improvements
//!
//! - Front matter (`---` delimited YAML-ish metadata on every published file)
//! - Paragraphs separated by `\n\n` (single `\n` within paragraphs preserved)
//! - ` thinking...  ` blocks stripped from output
//! - Clean markdown, no meta-commentary contamination

use std::collections::HashMap;
use std::path::PathBuf;

use roco_agent::mechanistic::{
    HandlerResult, MechanisticAgent, Plan as MechPlan, RepairConfig, Task,
};
use roco_engine::{CompletionRequest, ModelBackend};
use roco_grammar::{Schema, StrategyKind, StrategySelector};
use roco_tools::{ReadTool, Tool, WriteTool};
use roco_workspace::{Workspace, WorkspaceKind};
use serde::{Deserialize, Deserializer};
use serde_json::json;

use crate::{daemon, parse_opt};
use roco_app::agent_journal::AgentJournal;

// ═══════════════════════════════════════════════════════════════════════════
// Markdown helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Clean a generated text by stripping thinking blocks and ensuring proper
/// paragraph separation.
///
/// 1. Remove  thinking...  blocks (with or without closing tag)
/// 2. Strip trailing thinking blocks
/// 3. Ensure paragraphs are separated by `\n\n`, not single `\n`
fn clean_story_text(text: &str) -> String {
    let text = strip_thinking(text);
    let text = fix_paragraphs(&text);
    text.trim().to_string()
}

/// Strip  thinking...  and similar reasoning blocks from model output.
fn strip_thinking(text: &str) -> String {
    let mut result = String::new();
    let mut in_think = false;
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();

    while i < chars.len() {
        // Check for  thinking
        if i + 10 < chars.len()
            && chars[i] == '\u{1f4ad}'  // 💭
            && chars[i + 1] == ' '
            && chars[i + 2] == 't'
            && chars[i + 3] == 'h'
            && chars[i + 4] == 'i'
            && chars[i + 5] == 'n'
            && chars[i + 6] == 'k'
            && chars[i + 7] == 'i'
            && chars[i + 8] == 'n'
            && chars[i + 9] == 'g'
        {
            in_think = true;
            // Skip the entire  thinking marker
            i += 10; // 💭 + space + "thinking"
            continue;
        }

        // Check for closing  tag
        if in_think && i + 1 < chars.len() && chars[i] == '\u{1f4ad}' && chars[i + 1] == ' ' {
            in_think = false;
            i += 2; // 💭 + space
                    // Also skip anything that looks like "response" after it
            if i + 8 < chars.len()
                && chars[i] == 'r'
                && chars[i + 1] == 'e'
                && chars[i + 2] == 's'
                && chars[i + 3] == 'p'
                && chars[i + 4] == 'o'
                && chars[i + 5] == 'n'
                && chars[i + 6] == 's'
                && chars[i + 7] == 'e'
            {
                i += 8;
            }
            continue;
        }

        // Also handle plain "thinking" keyword start
        if !in_think
            && i + 9 < chars.len()
            && chars[i] == 't'
            && chars[i + 1] == 'h'
            && chars[i + 2] == 'i'
            && chars[i + 3] == 'n'
            && chars[i + 4] == 'k'
            && chars[i + 5] == 'i'
            && chars[i + 6] == 'n'
            && chars[i + 7] == 'g'
            && chars[i + 8] == '\n'
            && (i == 0 || chars[i - 1] == '\n')
        {
            in_think = true;
            i += 9; // "thinking\n"
            continue;
        }

        if !in_think {
            result.push(chars[i]);
        }
        i += 1;
    }

    result
}

/// Ensure paragraphs are separated by `\n\n`, not single `\n`.
///
/// A paragraph boundary is a blank line (two consecutive newlines with only
/// whitespace between them). Single newlines within a paragraph are preserved.
fn fix_paragraphs(text: &str) -> String {
    let mut result = String::new();
    let lines: Vec<&str> = text.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        if line.trim().is_empty() {
            // Preserve existing blank lines
            result.push_str("\n\n");
            i += 1;
            continue;
        }

        // Check if this line is the end of a paragraph (next line is blank
        // OR next line starts with capital letter and this line ends with period)
        let is_para_break = if i + 1 < lines.len() {
            let next = lines[i + 1].trim();
            if next.is_empty() {
                false // handled above
            } else if line.ends_with('.')
                || line.ends_with('!')
                || line.ends_with('?')
                || line.ends_with('"')
                || line.ends_with('”')
                || line.ends_with('—')
            {
                // If next line starts with a capital letter and this line
                // looks complete, it's likely a new paragraph
                next.starts_with(|c: char| c.is_uppercase() || c == '"' || c == '“' || c == '*')
                    && line.len() > 30
            } else {
                false
            }
        } else {
            false
        };

        if is_para_break || line.trim().starts_with('#') || line.trim().starts_with("---") {
            result.push_str(line.trim_end());
            result.push_str("\n\n");
        } else if !line.trim().is_empty() {
            if !result.is_empty() && !result.ends_with('\n') {
                result.push(' ');
            }
            if result.is_empty() || result.ends_with('\n') {
                result.push_str(line.trim_end());
            } else {
                result.push_str(line.trim_end());
            }
            result.push('\n');
        }

        i += 1;
    }

    // Clean up: replace multiple blank lines with a single blank line
    let mut cleaned = String::new();
    let mut prev_blank = false;
    for line in result.lines() {
        if line.trim().is_empty() {
            if !prev_blank {
                cleaned.push_str("\n\n");
                prev_blank = true;
            }
        } else {
            if !cleaned.is_empty() && !cleaned.ends_with('\n') && prev_blank {
                cleaned.push('\n');
            }
            if prev_blank && !cleaned.ends_with('\n') {
                cleaned.push('\n');
            }
            cleaned.push_str(line);
            cleaned.push('\n');
            prev_blank = false;
        }
    }

    let cleaned = cleaned.trim().to_string();

    // Final pass: ensure double-newlines between paragraphs
    let result = cleaned.replace("\n\n\n", "\n\n");
    result
}

/// Generate front matter for a story document.
fn front_matter(title: &str, genre: &str, tone: &str, word_count: usize) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Format timestamp as YYYY-MM-DD HH:MM:SS
    let h = (now / 3600) % 24;
    let m = (now / 60) % 60;
    let s = now % 60;
    let days = now / 86400;
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mo <= 2 { y + 1 } else { y };

    format!(
        "---\ntitle: \"{title}\"\ngenre: \"{genre}\"\ntone: \"{tone}\"\nword_count: {word_count}\ncreated_at: \"{yr:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}\"\n---\n\n"
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Story types (unchanged from original)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct StoryOutline {
    title: String,
    genre: String,
    tone: String,
    chapters: Vec<StoryChapterInfo>,
}

#[derive(Debug, Deserialize)]
struct StoryChapterInfo {
    number: u64,
    title: String,
    summary: String,
}

impl StoryOutline {
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
}

#[derive(Debug, Deserialize)]
struct StoryWiki {
    characters: Vec<StoryCharacter>,
    setting: String,
}

#[derive(Debug, Deserialize)]
struct StoryCharacter {
    name: String,
    description: String,
}

impl StoryWiki {
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
}

#[derive(Debug, Deserialize)]
struct StoryChapter {
    title: String,
    content: String,
}

impl StoryChapter {
    fn schema() -> Schema {
        Schema::object()
            .prop("title", Schema::string())
            .prop("content", Schema::string())
            .build()
    }
}

/// Helper: deserialize a field that might be a string OR an array of strings.
fn string_or_array<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de;
    struct StringOrArray;
    impl<'de> de::Visitor<'de> for StringOrArray {
        type Value = String;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or array of strings")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(v.to_string())
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut parts = Vec::new();
            while let Some(val) = seq.next_element::<String>()? {
                parts.push(val);
            }
            Ok(parts.join("; "))
        }
    }
    deserializer.deserialize_any(StringOrArray)
}

#[derive(Debug, Deserialize)]
struct StoryValidation {
    quality: String,
    #[serde(deserialize_with = "string_or_array")]
    issues: String,
    suggestion: String,
}

impl StoryValidation {
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
}

/// Synopsis that accepts either `summary` or `content` field.
#[derive(Debug, Deserialize)]
struct StorySynopsis {
    #[serde(alias = "content")]
    summary: String,
}

impl StorySynopsis {
    fn schema() -> Schema {
        Schema::object().prop("summary", Schema::string()).build()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// State-tuned structured completion: generates without grammar constraint,
/// then parses using `clean_json_output` (strips code fences, thinking blocks,
/// and extracts the first JSON object/array).
///
/// This is more reliable than grammar-constrained generation for RWKV models,
/// which have known issues with BNF grammar (thinking contamination, code fences).
fn structured_complete_with_strategy<T>(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    _strategy: &StrategySelector,
    temperature: f32,
    max_tokens: usize,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let text = futures::executor::block_on(backend.complete(CompletionRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        grammar: None, // No grammar constraint - state tuned approach
        temperature,
        max_tokens,
        prefill: Some("{\n".into()),
        ..Default::default()
    }))
    .map_err(|e| format!("model error: {e}"))?
    .text;

    // Parse using StateTunedStrategy's robust clean_json_output
    // Handles: code fences, thinking blocks, nested JSON
    let cleaned = roco_grammar::strategies::clean_json_output(&text);
    serde_json::from_str::<T>(&cleaned)
        .map_err(|e| format!("parse error: {e}\nraw: {text}\ncleaned: {cleaned}"))
}

fn extract_title(outline: &str) -> String {
    for line in outline.lines() {
        if line.starts_with("Title:") {
            return line.trim_start_matches("Title:").trim().to_string();
        }
    }
    "Untitled Story".to_string()
}

fn extract_genre(outline: &str) -> String {
    for line in outline.lines() {
        if line.starts_with("Genre:") {
            return line.trim_start_matches("Genre:").trim().to_string();
        }
    }
    "Unknown".to_string()
}

fn extract_tone(outline: &str) -> String {
    for line in outline.lines() {
        if line.starts_with("Tone:") {
            return line.trim_start_matches("Tone:").trim().to_string();
        }
    }
    "Unknown".to_string()
}

/// Create a slug from a title suitable for filenames.
fn title_to_slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .trim_matches('_')
        .to_string()
}

fn sanitize_story_dirname(s: &str) -> String {
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

fn create_story_workspace(prompt: &str) -> Result<Workspace, anyhow::Error> {
    let base = std::env::current_dir()?.join(".roco").join("workspaces");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = if prompt.trim().is_empty() {
        format!("story_ts_{ts}")
    } else {
        let words: Vec<&str> = prompt.split_whitespace().take(4).collect();
        format!("story_{}", sanitize_story_dirname(&words.join("_")))
    };
    let dir = base.join(format!("{name}_{ts}"));
    std::fs::create_dir_all(&dir)?;
    let ws = Workspace::from_existing(dir, WorkspaceKind::Agent)?;
    Ok(ws.with_name(name))
}

/// Publish the final story to `.roco/stories/{title}.md` as well.
fn publish_to_stories_dir(
    title: &str,
    genre: &str,
    tone: &str,
    story_text: &str,
) -> Result<PathBuf, String> {
    let slug = title_to_slug(title);
    let slug = if slug.is_empty() { "untitled" } else { &slug };

    let stories_dir = std::env::current_dir()
        .map_err(|e| format!("cwd: {e}"))?
        .join(".roco")
        .join("stories");

    std::fs::create_dir_all(&stories_dir).map_err(|e| format!("create stories dir: {e}"))?;

    let word_count = story_text.split_whitespace().count();
    let fm = front_matter(title, genre, tone, word_count);
    let full_content = format!("{fm}{story_text}");

    let path = stories_dir.join(format!("{slug}.md"));
    std::fs::write(&path, &full_content).map_err(|e| format!("write story file: {e}"))?;

    Ok(path)
}

/// Find an existing workspace related to the user's prompt, or create a new one.
/// An "active" workspace is stored in `.roco/workspaces/active`.
#[allow(dead_code)]
fn find_or_create_workspace(prompt: &str) -> Result<Workspace, anyhow::Error> {
    let base = std::env::current_dir()?.join(".roco").join("workspaces");

    // Check if there's an active workspace pointer
    let active_path = base.join("active");
    if active_path.exists() {
        if let Ok(active_name) = std::fs::read_to_string(&active_path) {
            let active_name = active_name.trim().to_string();
            let active_dir = base.join(&active_name);
            if active_dir.exists() {
                AgentJournal::info("story", &format!("Using existing workspace: {active_name}"));
                let ws = Workspace::from_existing(active_dir, WorkspaceKind::Agent)?;
                return Ok(ws.with_name(active_name));
            }
        }
    }

    // Create a new workspace
    let ws = create_story_workspace(prompt)?;
    let ws_name = ws.name().to_string();

    // Write active pointer
    std::fs::write(&active_path, &ws_name).ok();

    AgentJournal::action("story", &format!("Created workspace: {ws_name}"));

    Ok(ws)
}

// ═══════════════════════════════════════════════════════════════════════════
// Command entry point
// ═══════════════════════════════════════════════════════════════════════════

pub fn cmd_story(extra: &[&str]) {
    // Initialize the agent journal so components can log
    let _ = AgentJournal::init();

    let prompt = extra.first().cloned().unwrap_or(
        "Write a short story about a lighthouse keeper who discovers a message in a bottle.",
    );

    let strategy_str = parse_opt("--strategy", extra).unwrap_or("loose");
    let strategy_kind = StrategyKind::parse(strategy_str).unwrap_or(StrategyKind::StateTuned);

    let max_tok_str = parse_opt("--max-tokens", extra).unwrap_or("800");
    let max_tokens = max_tok_str.parse::<usize>().unwrap_or(800);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    let backend = daemon::ensure_backend();

    rt.block_on(async move {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .init();

        AgentJournal::phase("story", &format!("Starting story pipeline for: \"{prompt}\""));
        println!("Generating story...\n");

        let mut agent = MechanisticAgent::new()
            .with_repair(RepairConfig {
                max_retries: 2,
                temperature: 0.7,
                temperature_delta: 0.2,
                temperature_floor: 0.3,
                max_tokens,
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

        let outline_strategy =
            StrategySelector::new(strategy_kind, StoryOutline::schema(), "");
        let wiki_strategy =
            StrategySelector::new(strategy_kind, StoryWiki::schema(), "");
        let chapter_strategy =
            StrategySelector::new(strategy_kind, StoryChapter::schema(), "");
        let val_strategy =
            StrategySelector::new(strategy_kind, StoryValidation::schema(), "");
        let synopsis_strategy =
            StrategySelector::new(strategy_kind, StorySynopsis::schema(), "");

        // ── Workspace setup ───────────────────────────────────────────
        AgentJournal::info("story", "Setting up workspace...");
        let ws = create_story_workspace(prompt).unwrap();
        let workspace_path = ws.root().to_string_lossy().to_string();

        println!("  Workspace: {workspace_path}\n");
        println!("  Pipeline: outline → worldbuilding → chapters → validation → synopsis → publish\n");

        // ── Handler: compose/outline ──────────────────────────────────
        let outline_strategy_clone = outline_strategy;
        agent.register(
            "compose",
            "outline",
            Box::new(move |task, backend, ws| {
                let premise = task
                    .spec
                    .get("premise")
                    .and_then(|v| v.as_str())
                    .unwrap_or("a short story");

                AgentJournal::phase("story", "Generating outline (phase 1/6)...");

                let outline: StoryOutline =
                    structured_complete_with_strategy(
                        backend,
                        "You are a story outliner. Output valid JSON only. \
                         Do NOT include any thinking or reasoning. Output ONLY the JSON object.",
                        &format!(
                            "Outline a short story with 3 chapters based on this premise:\n{premise}\n\n\
                             Output JSON matching the schema: title, genre, tone, chapters \
                             (array of 3 objects with number, title, summary).",
                        ),
                        &outline_strategy_clone,
                        0.6,
                        300,
                    )
                    .unwrap_or_else(|e| StoryOutline {
                        title: "Untitled".into(),
                        genre: "Unknown".into(),
                        tone: "Unknown".into(),
                        chapters: (1..=3)
                            .map(|i| StoryChapterInfo {
                                number: i,
                                title: format!("Chapter {i}"),
                                summary: format!("Error generating outline: {e}"),
                            })
                            .collect(),
                    });

                // Build formatted markdown with front matter
                let title = &outline.title;
                let genre = &outline.genre;
                let tone = &outline.tone;
                let word_count = 0; // outline doesn't have prose yet

                let mut md = front_matter(title, genre, tone, word_count);
                md.push_str(&format!("# {}\n\n**Genre:** {}  \n**Tone:** {}\n\n", title, genre, tone));
                for ch in &outline.chapters {
                    md.push_str(&format!(
                        "## Chapter {}: {}\n\n{}\n\n",
                        ch.number, ch.title, ch.summary
                    ));
                }

                // Write outline
                let path = ws.resolve("01-OUTLINE.md").unwrap();
                let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

                AgentJournal::action("story", &format!(
                    "Outline complete: \"{title}\" - {} chapters ({genre}, {tone})",
                    outline.chapters.len()
                ));

                HandlerResult {
                    task: task.clone(),
                    output: format!(
                        "Title: {title}\nGenre: {genre}\nTone: {tone}\nChapters: {}\n",
                        outline.chapters.len()
                    ),
                    files: HashMap::new(),
                    pass: true,
                }
            }),
        );

        // ── Handler: compose/wiki ────────────────────────────────────
        let wiki_strategy_clone = wiki_strategy;
        agent.register(
            "compose",
            "wiki",
            Box::new(move |task, backend, ws| {
                let premise = task
                    .spec
                    .get("premise")
                    .and_then(|v| v.as_str())
                    .unwrap_or("a short story");
                let outline = task
                    .spec
                    .get("outline")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                AgentJournal::phase("story", "Building world bible (phase 2/6)...");

                let wiki: StoryWiki = structured_complete_with_strategy(
                    backend,
                    "You are a worldbuilding assistant. Output valid JSON only. \
                     No thinking, no reasoning, no commentary. Only JSON.",
                    &format!(
                        "Based on this premise and outline, create character bios and setting lore:\n\n\
                         Premise: {premise}\nOutline: {outline}\n\n\
                         Output JSON matching the schema: characters (array of objects with name, description), \
                         setting (string).",
                    ),
                    &wiki_strategy_clone,
                    0.7,
                    500,
                )
                .unwrap_or_else(|e| StoryWiki {
                    characters: vec![StoryCharacter {
                        name: "Unknown".into(),
                        description: format!("Error generating wiki: {e}"),
                    }],
                    setting: "Unknown".into(),
                });

                let mut md = String::from("# World Bible\n\n");
                md.push_str("## Characters\n\n");
                for ch in &wiki.characters {
                    md.push_str(&format!("### {}\n\n{}\n\n", ch.name, ch.description));
                }
                md.push_str("## Setting\n\n");
                md.push_str(&format!("{}\n", wiki.setting));

                let path = ws.resolve("02-WIKI.md").unwrap();
                let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

                AgentJournal::action("story", &format!(
                    "World bible: {} characters created",
                    wiki.characters.len()
                ));

                HandlerResult {
                    task: task.clone(),
                    output: md,
                    files: HashMap::new(),
                    pass: true,
                }
            }),
        );

        // ── Handler: write/chapter ────────────────────────────────────
        let chapter_strategy_clone = chapter_strategy;
        agent.register(
            "write",
            "chapter",
            Box::new(move |task, backend, ws| {
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
                let is_retry = task
                    .spec
                    .get("retry")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let label = if is_retry {
                    format!("{chapter_label} (revision)")
                } else {
                    chapter_label.to_string()
                };

                AgentJournal::phase("story", &format!("Writing {label} (phase 3/6)..."));

                let directive = if chapter_num == 1 {
                    format!(
                        "Write {chapter_label}. Introduce the main character and setting. \
                         ~400 words of vivid prose.\n\n\
                         Rules:\n\
                         - Write actual story prose, NOT meta-commentary or planning.\n\
                         - Start directly with the narrative.\n\
                         - Use paragraph breaks (double newlines) between scenes.\n\
                         - Do NOT include thinking, reasoning, or commentary.\n\n\
                         Outline context:\n{outline}\n\n\
                         Output JSON with: title (string), content (string, the chapter prose)",
                    )
                } else {
                    format!(
                        "Write {chapter_label}. Continue from where the previous chapter left off. \
                         Advance the plot. ~400 words of vivid prose.\n\n\
                         Rules:\n\
                         - Write actual story prose, NOT meta-commentary or planning.\n\
                         - Start directly with the narrative.\n\
                         - Use paragraph breaks (double newlines) between scenes.\n\
                         - Do NOT include thinking, reasoning, or commentary.\n\n\
                         Previous chapter recap:\n{previous}\n\n\
                         Outline context:\n{outline}\n\n\
                         Output JSON with: title (string), content (string, the chapter prose)",
                    )
                };

                let chapter: StoryChapter = structured_complete_with_strategy(
                    backend,
                    "You are a fiction writer. Write vivid, engaging prose. \
                     Output valid JSON only. NEVER include thinking, reasoning, \
                     or meta-commentary in your output. Only the JSON object.",
                    &directive,
                    &chapter_strategy_clone,
                    0.8,
                    max_tokens,
                )
                .unwrap_or_else(|e| StoryChapter {
                    title: chapter_label.into(),
                    content: format!("Error writing chapter: {e}"),
                });

                // Clean the content: strip thinking, fix paragraphs
                let clean_content = clean_story_text(&chapter.content);

                // Build markdown with front matter
                let md = format!("# {}\n\n{}", chapter.title, clean_content);

                let filename = format!("03-CHAPTER_{chapter_num}.md");
                let path = ws.resolve(&filename).unwrap();
                let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

                let wc = clean_content.split_whitespace().count();
                AgentJournal::action("story", &format!(
                    "{label}: {wc} words written to {filename}"
                ));

                HandlerResult {
                    task: task.clone(),
                    output: md,
                    files: HashMap::new(),
                    pass: true,
                }
            }),
        );

        // ── Handler: validate/chapter ─────────────────────────────────
        let val_strategy_clone = val_strategy;
        agent.register(
            "validate",
            "chapter",
            Box::new(move |task, backend, ws| {
                let chapter_text = task
                    .spec
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let chapter_num = task
                    .spec
                    .get("number")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let entry = if chapter_text.trim().is_empty() {
                    format!("\n## Chapter {chapter_num}\n\n[validation skipped — chapter is empty]\n")
                } else {
                    structured_complete_with_strategy::<StoryValidation>(
                        backend,
                        "You are a quality reviewer. Be strict. Output valid JSON only. \
                         Check for meta-commentary and thinking contamination.",
                        &format!(
                            "Review this chapter and check for:\n\
                             1. Does it read like a coherent story (not meta-commentary)?\n\
                             2. Is the prose engaging with proper paragraph breaks?\n\
                             3. Does it avoid thinking/reasoning tags?\n\n\
                             Chapter:\n{chapter_text}\n\n\
                             Output JSON matching the schema: quality (\"pass\" | \"fail\" | \"needs-work\"), \
                             issues (string), suggestion (string).",
                        ),
                        &val_strategy_clone,
                        0.3,
                        200,
                    )
                    .map(|v: StoryValidation| {
                        format!(
                            "\n## Chapter {chapter_num}\n\nQuality: {}\nIssues: {}\nSuggestion: {}\n",
                            v.quality, v.issues, v.suggestion
                        )
                    })
                    .unwrap_or_else(|e| {
                        format!(
                            "\n## Chapter {chapter_num}\n\nQuality: fail\nIssues: Model error: {e}\nSuggestion: Retry\n"
                        )
                    })
                };

                // Log quality result
                if entry.contains("Quality: pass") {
                    AgentJournal::info("story", &format!("Chapter {chapter_num} quality: PASS"));
                } else {
                    AgentJournal::warn("story", &format!("Chapter {chapter_num} quality: ISSUES FOUND"));
                }

                let path = ws.resolve("04-VALIDATION.md").unwrap();
                let existing = ReadTool
                    .call(json!({"path": path.to_string_lossy()}))
                    .ok()
                    .and_then(|v| {
                        v.get("content")
                            .and_then(|c| c.as_str().map(String::from))
                    })
                    .unwrap_or_default();
                let _ = WriteTool.call(json!({
                    "path": path.to_string_lossy(),
                    "content": existing + &entry,
                }));

                HandlerResult {
                    task: task.clone(),
                    output: entry,
                    files: HashMap::new(),
                    pass: true,
                }
            }),
        );

        // ── Handler: write/synopsis ───────────────────────────────────
        let synopsis_strategy_clone = synopsis_strategy;
        agent.register(
            "write",
            "synopsis",
            Box::new(move |task, backend, ws| {
                let chapters = task
                    .spec
                    .get("chapters")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                AgentJournal::phase("story", "Writing synopsis (phase 5/6)...");

                let synopsis: StorySynopsis = structured_complete_with_strategy(
                    backend,
                    "You are a literary summarizer. Output valid JSON only. \
                     No thinking, no reasoning.",
                    &format!(
                        "Write a one-paragraph synopsis of the complete story based on these chapters:\n\n\
                         {chapters}\n\n\
                         Output JSON matching the schema: summary (string, one paragraph, ~100 words)",
                    ),
                    &synopsis_strategy_clone,
                    0.5,
                    200,
                )
                .unwrap_or_else(|e| StorySynopsis {
                    summary: format!("Error writing synopsis: {e}"),
                });

                let md = format!("# Synopsis\n\n{}", synopsis.summary);

                let path = ws.resolve("05-SYNOPSIS.md").unwrap();
                let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

                AgentJournal::action("story", "Synopsis complete");

                HandlerResult {
                    task: task.clone(),
                    output: md,
                    files: HashMap::new(),
                    pass: true,
                }
            }),
        );

        // ── Handler: publish/chapter ──────────────────────────────────
        agent.register(
            "publish",
            "chapter",
            Box::new(|_task, _backend, ws| {
                AgentJournal::phase("story", "Publishing (phase 6/6)...");

                let read_file = |name: &str| -> String {
                    ReadTool
                        .call(json!({"path": ws.root().join(name).to_string_lossy()}))
                        .ok()
                        .and_then(|v| {
                            v.get("content")
                                .and_then(|c| c.as_str().map(String::from))
                        })
                        .unwrap_or_default()
                };
                let outline = read_file("01-OUTLINE.md");
                let title = extract_title(&outline);
                let genre = extract_genre(&outline);
                let tone = extract_tone(&outline);

                // Compile the full story
                let mut story = String::new();

                // Characters & Setting section
                let wiki = read_file("02-WIKI.md");
                if !wiki.is_empty() {
                    story.push_str(&wiki);
                    story.push_str("\n\n---\n\n");
                }

                // Chapters
                for i in 1..=3 {
                    let ch = ReadTool
                        .call(json!({"path": ws.root().join(format!("03-CHAPTER_{i}.md")).to_string_lossy()}))
                        .ok()
                        .and_then(|v| {
                            v.get("content")
                                .and_then(|c| c.as_str().map(String::from))
                        })
                        .unwrap_or_default();
                    if !ch.is_empty() {
                        story.push_str(&ch);
                        story.push_str("\n\n---\n\n");
                    }
                }

                // Synopsis
                let synopsis = read_file("05-SYNOPSIS.md");
                if !synopsis.is_empty() {
                    story.push_str(&synopsis);
                    story.push_str("\n");
                }

                // Write 06-STORY.md to workspace (with front matter)
                let word_count = story.split_whitespace().count();
                let fm = front_matter(&title, &genre, &tone, word_count);
                let full_story = format!("{fm}{story}");

                let path = ws.resolve("06-STORY.md").unwrap();
                let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &full_story}));

                // Also publish to .roco/stories/{slug}.md
                match publish_to_stories_dir(&title, &genre, &tone, &story) {
                    Ok(story_path) => {
                        AgentJournal::action("story", &format!(
                            "Published to {} ({} words)",
                            story_path.display(),
                            word_count
                        ));
                        println!("\n  📖 Published: {}", story_path.display());
                    }
                    Err(e) => {
                        AgentJournal::warn("story", &format!("Failed to publish to stories dir: {e}"));
                    }
                }

                AgentJournal::action("story", &format!(
                    "Story complete! \"{title}\" — {word_count} words, 3 chapters"
                ));

                HandlerResult {
                    task: Task {
                        r#type: "publish".into(),
                        domain: "chapter".into(),
                        spec: serde_json::json!({"status": "published"}),
                    },
                    output: format!(
                        "Published: {title} ({word_count} words, 3 chapters)"
                    ),
                    files: HashMap::new(),
                    pass: true,
                }
            }),
        );

        // ═══════════════════════════════════════════════════════════════
        // Execution
        // ═══════════════════════════════════════════════════════════════

        // Phase 1: outline
        println!("📝 Outline...");
        AgentJournal::info("story", "Phase 1: Generating outline");
        let plan = MechPlan {
            tasks: vec![Task {
                r#type: "compose".into(),
                domain: "outline".into(),
                spec: serde_json::json!({"premise": prompt}),
            }],
        };
        let outline_result = agent
            .dispatch_single(backend.as_ref(), &plan.tasks[0], &ws)
            .expect("outline failed");
        let outline_text = &outline_result.output;
        println!("  ✓ Outline complete\n");

        // Phase 2: wiki
        println!("📚 Worldbuilding...");
        AgentJournal::info("story", "Phase 2: Building world bible");
        let wiki_plan = MechPlan {
            tasks: vec![Task {
                r#type: "compose".into(),
                domain: "wiki".into(),
                spec: serde_json::json!({"premise": prompt, "outline": outline_text}),
            }],
        };
        let _wiki_result = agent
            .dispatch_single(backend.as_ref(), &wiki_plan.tasks[0], &ws)
            .expect("wiki failed");
        println!("  ✓ World bible complete\n");

        // Phase 3: chapters ×3
        AgentJournal::info("story", "Phase 3: Writing chapters");
        let mut chapter_texts = Vec::new();
        for i in 1..=3 {
            let chapter_label = format!("Chapter {i}");
            let previous = chapter_texts.last().cloned().unwrap_or_default();

            println!("  ✍️  {}...", &chapter_label);

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
                .dispatch_single(backend.as_ref(), &ch_task, &ws)
                .expect("chapter failed");
            chapter_texts.push(ch_result.output.clone());

            // Validation
            println!("  🔍 Validating {}...", &chapter_label);
            let val_task = Task {
                r#type: "validate".into(),
                domain: "chapter".into(),
                spec: serde_json::json!({
                    "number": i,
                    "text": ch_result.output,
                }),
            };
            let _val_result = agent
                .dispatch_single(backend.as_ref(), &val_task, &ws)
                .expect("validation failed");

            // Self-correction loop
            let val_path = ws.root().join("04-VALIDATION.md");
            if let Some(val_content) = ReadTool
                .call(json!({"path": val_path.to_string_lossy()}))
                .ok()
                .and_then(|v| v.get("content").and_then(|c| c.as_str().map(String::from)))
            {
                let chapter_header = format!("## Chapter {i}");
                let needs_revision = if let Some(start_idx) =
                    val_content.find(&chapter_header)
                {
                    let segment = &val_content[start_idx..];
                    let next_chapter_header = format!("## Chapter {}", i + 1);
                    let segment = if let Some(end_idx) = segment.find(&next_chapter_header)
                    {
                        &segment[..end_idx]
                    } else {
                        segment
                    };
                    segment.contains("Quality: fail")
                        || segment.contains("Quality: needs-work")
                } else {
                    false
                };

                if needs_revision {
                    println!("  ⚠️  {} needs revision — retrying...", &chapter_label);
                    AgentJournal::warn("story", &format!("{chapter_label} needs revision, retrying..."));

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
                        .dispatch_single(backend.as_ref(), &retry_task, &ws)
                        .unwrap_or(ch_result);

                    let filename = format!("03-CHAPTER_{i}.md");
                    let path = ws.resolve(&filename).unwrap();
                    let _ = WriteTool.call(json!({
                        "path": path.to_string_lossy(),
                        "content": &retry_result.output,
                    }));
                    chapter_texts[i - 1] = retry_result.output;
                    println!("  ✓ {chapter_label} revised\n");
                } else {
                    println!("  ✓ {chapter_label} quality check passed\n");
                }
            }
        }

        // Phase 4: synopsis
        println!("📋 Synopsis...");
        AgentJournal::info("story", "Phase 5: Writing synopsis");
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
            .dispatch_single(backend.as_ref(), &synopsis_task, &ws)
            .expect("synopsis failed");
        println!("  ✓ Synopsis complete\n");

        // Phase 5: publish
        println!("📦 Publishing...");
        AgentJournal::info("story", "Phase 6: Publishing");
        let publish_task = Task {
            r#type: "publish".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({}),
        };
        let publish_result = agent
            .dispatch_single(backend.as_ref(), &publish_task, &ws)
            .expect("publish failed");

        let outcome = agent
            .commit(
                plan.clone(),
                vec![outline_result, publish_result],
                &ws,
            )
            .unwrap();

        println!(
            "✅ Done! {} files in workspace:\n",
            outcome.workspace_files.len()
        );
        let mut filenames: Vec<_> = outcome.workspace_files.keys().collect();
        filenames.sort();
        for fname in &filenames {
            let size = outcome.workspace_files[*fname].len();
            println!("  📄 {fname} ({size} bytes)");
        }

        println!(
            "\n✅ Story published to {}",
            workspace_path
        );
        println!("✅ Journal: .roco/agent-journal.md");
        println!(
            "✅ Monitor: tail -f .roco/agent-journal.md\n"
        );
    });
}
