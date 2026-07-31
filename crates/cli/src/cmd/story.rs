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
use roco_agent::AgentError;
use roco_engine::{CompletionRequest, ModelBackend};

use roco_agent::tools::{ReadTool, Tool, WriteTool};
use roco_engine::grammar::{Schema, StrategyKind, StrategySelector};
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
            result.push_str(line.trim_end());
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
    cleaned.replace("\n\n\n", "\n\n")
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
    #[serde(deserialize_with = "string_or_setting_object")]
    setting: String,
}

/// Accept either a plain string or an object with name/description fields.
/// Model sometimes outputs `{"name": "...", "description": "..."}` instead of a string.
fn string_or_setting_object<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or object with name/description")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<String, A::Error> {
            let mut name = String::new();
            let mut description = String::new();
            while let Some((key, value)) = map.next_entry::<String, String>()? {
                match key.as_str() {
                    "name" => name = value,
                    "description" => description = value,
                    _ => {}
                }
            }
            Ok(format!("{name}: {description}"))
        }
    }
    deserializer.deserialize_any(Visitor)
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct StoryCharacter {
    name: String,
    description: String,
    role: Option<String>,
    #[serde(default, deserialize_with = "string_or_setting_object")]
    setting: String,
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
// Session names — separate roles so state doesn't bleed between them
// ═══════════════════════════════════════════════════════════════════════════

/// Session for outline, wiki, chapter writing, synopsis (narrative generation)
const SESSION_WRITER: &str = "story-writer";
/// Session for quality validation (different output format, must not carry writer state)
const SESSION_VALIDATOR: &str = "story-validator";

// ═══════════════════════════════════════════════════════════════════════════
// Prompts — all at module level for visibility and easy editing
// ═══════════════════════════════════════════════════════════════════════════

/// System prompt for outline generation.
const SYSTEM_OUTLINE: &str = "You are a story outliner. Output valid JSON only. \
Do NOT include any thinking or reasoning. Output ONLY the JSON object.";

/// System prompt for wiki/worldbuilding.
const SYSTEM_WIKI: &str = "You are a worldbuilding assistant. Output valid JSON only. \
No thinking, no reasoning, no commentary. Only JSON.";

/// System prompt for chapter writing.
const SYSTEM_CHAPTER: &str = "You are a fiction writer. Output valid JSON only. \
The JSON must have 'title' (string) and 'content' (string with the story prose). \
No thinking, no reasoning, no commentary. Only the JSON object.";

/// System prompt for validation/review.
const SYSTEM_VALIDATOR: &str = "You are a quality reviewer. Be strict. Output valid JSON only. \
Check for meta-commentary and thinking contamination.";

/// System prompt for synopsis.
const SYSTEM_SYNOPSIS: &str = "You are a literary summarizer. Output valid JSON only. \
No thinking, no reasoning.";

/// Return the .roco directory, respecting $ROCO_DIR env var.
fn roco_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ROCO_DIR") {
        PathBuf::from(dir)
    } else {
        std::env::current_dir().unwrap_or_default().join(".roco")
    }
}

/// Prompt template for outline generation.
fn prompt_outline(premise: &str) -> String {
    format!(
        "Outline a short story with 3 chapters based on this premise:\n{premise}\n\n\
         Output JSON matching the schema: title, genre, tone, chapters \
         (array of 3 objects with number, title, summary)."
    )
}

/// Prompt template for wiki generation.
fn prompt_wiki(premise: &str, outline: &str) -> String {
    format!(
        "Based on this premise and outline, create character bios and setting lore:\n\n\
         Premise: {premise}\nOutline: {outline}\n\n\
         Output JSON matching the schema: characters (array of objects with name, description), \
         setting (string)."
    )
}

/// Read a file from a workspace, returning None if it doesn't exist.
fn read_ws_file(ws: &roco_workspace::Workspace, name: &str) -> Option<String> {
    let path = ws.root().join(name);
    if path.exists() {
        std::fs::read_to_string(&path).ok()
    } else {
        None
    }
}

/// Detect which chapters exist in a workspace (returns sorted chapter numbers).
fn detect_chapters(ws: &roco_workspace::Workspace) -> Vec<usize> {
    let mut chapters = Vec::new();
    if let Ok(entries) = std::fs::read_dir(ws.root()) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("03-CHAPTER_") && name.ends_with(".md") {
                if let Some(num_str) = name
                    .strip_prefix("03-CHAPTER_")
                    .and_then(|s| s.strip_suffix(".md"))
                {
                    if let Ok(num) = num_str.parse::<usize>() {
                        chapters.push(num);
                    }
                }
            }
        }
    }
    chapters.sort();
    chapters
}

/// Find the latest story workspace directory.
fn find_latest_workspace() -> Option<roco_workspace::Workspace> {
    let base = roco_dir().join("workspaces");
    if !base.exists() {
        return None;
    }
    let mut entries: Vec<_> = std::fs::read_dir(&base)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries.into_iter().next().map(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        // Use absolute path so workspace resolve works correctly
        let abs_path = e.path().canonicalize().unwrap_or_else(|_| e.path());
        roco_workspace::Workspace::from_existing(abs_path, WorkspaceKind::Agent)
            .unwrap()
            .with_name(name)
    })
}

/// Prompt template for chapter writing (first attempt).
///
/// The 2.9B model needs extremely explicit instructions.
/// We show the exact JSON structure and say "output ONLY this".
fn prompt_chapter(
    label: &str,
    title: &str,
    summary: &str,
    outline: &str,
    _previous: &str,
    is_first: bool,
) -> String {
    let context = if is_first {
        String::new()
    } else {
        format!("\nThis is {label}: {title}. Continue the story from where the previous chapter ended.\n")
    };
    format!(
        "Write a chapter for this story.\n\n\
         Scene: {summary}\n\n\
         Story outline:\n{outline}\n{context}\n\
         Rules:\n\
         - Write vivid prose, 300-500 words.\n\
         - Start with action or dialogue, not planning.\n\
         - Use paragraph breaks between scenes.\n\n\
         Output ONLY a JSON object. No other text.\n\
         The JSON must have exactly two keys: title and content.\n\n\
         Example:\n\
         {{\"title\": \"Chapter Title\", \"content\": \"The full story text here...\"}}"
    )
}

/// Prompt template for chapter revision (retry with feedback).
fn prompt_revision(
    label: &str,
    title: &str,
    summary: &str,
    feedback: &str,
    outline: &str,
) -> String {
    format!(
        "Revise {label}: {title} to fix the following issues:\n{feedback}\n\n\
         Chapter purpose:\n{summary}\n\n\
         Rules:\n\
         - Fix the specific issues listed above.\n\
         - Keep the original story elements that work.\n\
         - Write actual story prose, NOT meta-commentary or planning.\n\
         - Start directly with the narrative.\n\
         - Use paragraph breaks (double newlines) between scenes.\n\
         - Do NOT include thinking, reasoning, or commentary.\n\n\
         Full story outline:\n{outline}\n\n\
         Output ONLY a JSON object. No other text.\n\
         The JSON must have exactly two keys: title and content.\n\n\
         Example:\n\
         {{\"title\": \"Chapter Title\", \"content\": \"The full story text here...\"}}"
    )
}

/// Prompt template for validation.
fn prompt_validation(chapter_text: &str) -> String {
    format!(
        "Review this chapter and check for:\n\
         1. Does it read like a coherent story (not meta-commentary)?\n\
         2. Is the prose engaging with proper paragraph breaks?\n\
         3. Does it avoid thinking/reasoning tags?\n\n\
         Chapter:\n{chapter_text}\n\n\
         Output JSON matching the schema: quality (\"pass\" | \"fail\" | \"needs-work\"), \
         issues (string), suggestion (string)."
    )
}

/// Prompt template for synopsis.
fn prompt_synopsis(chapters: &str) -> String {
    format!(
        "Based on these chapter texts, write a one-paragraph synopsis (~100 words):\n\n\
{chapters}\n\n\
Output a JSON object with exactly one key: 'summary' (string)."
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Instructional bake examples — primes the MODEL'S OUTPUT FORMAT, not content
// ═══════════════════════════════════════════════════════════════════════════
//
// These examples teach the model to respond with JSON `{{title, content}}`
// when given a chapter-writing prompt. The THEMES must be different across
// examples so the state learns the FORMAT, not the CONTENT.
//
// The prompt side mirrors the actual chapter prompt structure
// (outline context + rules + JSON instruction) so the model's
// recurrent state expects that shape.

/// Bake examples for chapter-writing — diverse themes, same JSON output format.
const BAKE_CHAPTER_EXAMPLES: &[(&str, &str)] = &[
    (
        // Example 1: fantasy — no overlap with real story
        "Write Chapter 1: The Iron Gate. A blacksmith discovers a door beneath the forge.\n\n\
         ~400 words of vivid prose.\n\n\
         Rules:\n\
         - Write actual story prose, NOT meta-commentary.\n\
         - Start directly with the narrative.\n\
         - Use paragraph breaks between scenes.\n\n\
         Full story outline:\n\
         ## Chapter 1: The Iron Gate\nSmith finds a buried door.\n\
         ## Chapter 2: The Whispering Hall\nHe explores tunnels beneath.\n\
         ## Chapter 3: The Crown\nHe claims a forgotten legacy.\n\n\
         Output JSON with: title (string), content (string, the chapter prose)",
        r##"{"title": "The Iron Gate", "content": "Kael's hammer rang against the anvil long after the bellows fell cold. Three strikes, then a pause. Three more, then he set down the tool and pressed his ear to the flagstone floor. A hum — faint, rhythmic, like a heartbeat buried in stone. He reached for the chisel. The first chip of stone fell away, and a breath of cold air escaped from the crack, smelling of iron and old earth. Kael did not stop. He could not stop. Something down there was waiting."}"##
    ),
    (
        // Example 2: sci-fi — different domain, same format
        "Write Chapter 1: First Contact. Dr. Varma checks the deep-space radio array and finds a signal.\n\n\
         ~400 words of vivid prose.\n\n\
         Rules:\n\
         - Write actual story prose, NOT meta-commentary.\n\
         - Start directly with the narrative.\n\
         - Use paragraph breaks between scenes.\n\n\
         Full story outline:\n\
         ## Chapter 1: First Contact\nVarma detects an artificial signal from Proxima.\n\
         ## Chapter 2: Decoding\nThe message reveals a warning.\n\
         ## Chapter 3: The Reply\nHumanity must decide its answer.\n\n\
         Output JSON with: title (string), content (string, the chapter prose)",
        r##"{"title": "First Contact", "content": "The readout spooled across the monitor in flat green lines. Dr. Varma had been staring at it for eleven hours, long enough that the numbers had begun to blur into meaningless patterns. She reached for her coffee, found the mug empty, and set it down. That was when the signal arrived. Not the usual thermal noise or the rhythmic pulse of a pulsar. This was structured — prime numbers, clean and deliberate, repeating every 27.3 seconds. She pressed the intercom. \"Control, this is Varma. You need to hear this.\" The word caught in her throat. \"We are not alone.\"}"##
    ),
];

/// Bake examples for validation — teaches the model to output `{quality, issues, suggestion}` instead of narrative.
const BAKE_VALIDATION_EXAMPLES: &[(&str, &str)] = &[
    (
        "Review this chapter:\n\
         The dragon landed on the tower with a thud. Sir Garret drew his sword and charged. The end.\n\n\
         Output JSON matching the schema: quality (\"pass\" | \"fail\" | \"needs-work\"), \
         issues (string), suggestion (string).",
        r##"{"quality": "needs-work", "issues": "Chapter is extremely short (16 words). No character development, no setting, no tension building. Reads more like a plot summary than a story chapter.", "suggestion": "Expand to at least 300 words. Add sensory details about the dragon, the tower, the knight's emotions. Build tension before the confrontation."}"##
    ),
    (
        "Review this chapter:\n\
         So I was thinking about what should happen next and maybe the hero should find a map or something. Anyway, here is what I came up with: The hero walked into the cave. It was dark. The end.\n\n\
         Output JSON matching the schema: quality (\"pass\" | \"fail\" | \"needs-work\"), \
         issues (string), suggestion (string).",
        r##"{"quality": "fail", "issues": "Contains meta-commentary ('So I was thinking about what should happen next'). Prose is bare minimum with no sensory detail. Authorial intrusion breaks narrative immersion.", "suggestion": "Remove all meta-commentary. Rewrite with concrete sensory details: the cold damp of the cave, the echo of footsteps, the smell of moss and stone. Show the hero's emotional state through action."}"##
    ),
];

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Structured completion using the strategy's grammar + parser.
///
/// Grammar is passed as a string — `roco-inferd`'s server route builds
/// the `BnfMask` from it server-side via `create_bnf_mask`. Client-side
/// BnfMask construction is not possible (kbnf types trigger E0275 when
/// alongside web-rwkv in the same crate).
///
/// Sets `prefill` to `"{\n"` for grammar strategies to jump-start JSON.
/// For StateTuned, both `grammar` and `prefill` are left empty.
fn repair_json(s: &str) -> String {
    let mut text = s.trim();
    if let Some(start) = text.find("```json") {
        if let Some(end) = text[start + 7..].find("```") {
            text = text[start + 7..start + 7 + end].trim();
        }
    } else if let Some(start) = text.find("```") {
        if let Some(end) = text[start + 3..].find("```") {
            text = text[start + 3..start + 3 + end].trim();
        }
    }

    let mut result = String::with_capacity(text.len() + 32);
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;

    while i < n {
        let c = chars[i];
        if in_string {
            result.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if c == '"' {
            in_string = true;
            result.push(c);
            i += 1;
            continue;
        }

        result.push(c);

        if c == '}' {
            let mut j = i + 1;
            while j < n && chars[j].is_whitespace() {
                j += 1;
            }
            if j < n && chars[j] == '{' {
                result.push(',');
            }
        }
        i += 1;
    }
    result
}

/// Try to parse a prose outline (model output) into a StoryOutline.
///
/// The 2.9B model often writes narrative prose instead of JSON.
/// This function extracts structure from the model's natural format:
/// - ### Title: ...
/// - ### Genre: ...
/// # Prose fallback parsers — DELETED
///
/// These were added as a workaround for the 2.9B model's inability to
/// reliably produce JSON. They masked implementation bugs that should
/// have been fixed through evals-first methodology.
///
/// If JSON parsing fails, the phase should fail loudly — the fix is
/// to tune prompts/baking/grammar until the model produces valid JSON,
/// not to silently fall back to natural language heuristics.
///
/// Deleted functions: prose_to_outline, prose_to_wiki, prose_to_chapter,
/// prose_to_synopsis, prose_to_validation. See git history for the original
/// implementations.

#[allow(clippy::too_many_arguments)]
fn structured_complete_with_strategy<T>(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    strategy: &StrategySelector,
    temperature: f32,
    max_tokens: usize,
    session_id: Option<&str>,
    seed: Option<u64>,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let grammar = strategy.grammar();
    let use_grammar = !grammar.is_empty();
    let mut last_err = String::new();

    for _attempt in 0..3 {
        // Format system + prompt into raw text. inferd does NOT add
        // System/User/Assistant formatting — the gateway owns this.
        let prompt_text = if system.is_empty() {
            prompt.to_string()
        } else {
            format!(
                "System: {}\n\nUser: {}\n\nAssistant:",
                system.trim(),
                prompt
            )
        };
        let text_res = futures::executor::block_on(backend.complete(CompletionRequest {
            prompt: prompt_text,
            grammar: if use_grammar {
                Some(grammar.clone())
            } else {
                None
            },
            temperature,
            max_tokens,
            prefill: if use_grammar {
                Some("{\n".into())
            } else {
                None
            },
            session: session_id.map(|s| s.to_string()),
            seed,
            bnf_mask: None,
            ..Default::default()
        }));

        let text = match text_res {
            Ok(resp) => resp.text,
            Err(e) => {
                last_err = format!("model error: {e}");
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }
        };

        match strategy.parse::<T>(&text) {
            Ok(val) => return Ok(val),
            Err(orig_e) => {
                let repaired = repair_json(&text);
                match strategy.parse::<T>(&repaired) {
                    Ok(val) => return Ok(val),
                    Err(_) => {
                        // No prose fallback — phase fails loudly.
                        // If the model can't produce valid JSON, fix the
                        // prompts/baking/grammar, don't silently fall back.
                        last_err = format!("parse error: {orig_e}\nraw: {text}");
                    }
                }
            }
        }
    }
    Err(last_err)
}

fn extract_title(outline: &str) -> String {
    for line in outline.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed
            .strip_prefix("title:")
            .or_else(|| trimmed.strip_prefix("Title:"))
        {
            return val.trim().trim_matches('"').to_string();
        }
    }
    "Untitled Story".to_string()
}

/// Parse the outline markdown and extract the title and summary for a specific chapter.
fn chapter_outline_info(outline: &str, chapter_num: usize) -> (String, String) {
    let header = format!("## Chapter {}: ", chapter_num);
    if let Some(start) = outline.find(&header) {
        let rest = &outline[start + header.len()..];
        // Find end of title line
        let title_end = rest.find('\n').unwrap_or(rest.len());
        let title = rest[..title_end].trim().to_string();
        // Find summary text (after title line, before next ## or end)
        let after_title = &rest[title_end..].trim_start();
        let summary = if let Some(next_header) = after_title.find("\n## ") {
            after_title[..next_header].trim().to_string()
        } else {
            after_title.trim().to_string()
        };
        (title, summary)
    } else {
        (format!("Chapter {chapter_num}"), String::new())
    }
}
fn extract_genre(outline: &str) -> String {
    for line in outline.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed
            .strip_prefix("genre:")
            .or_else(|| trimmed.strip_prefix("Genre:"))
        {
            return val.trim().trim_matches('"').to_string();
        }
    }
    "Unknown".to_string()
}

fn extract_tone(outline: &str) -> String {
    for line in outline.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed
            .strip_prefix("tone:")
            .or_else(|| trimmed.strip_prefix("Tone:"))
        {
            return val.trim().trim_matches('"').to_string();
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
    let base = roco_dir().join("workspaces");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let slug = if prompt.trim().is_empty() {
        "story".to_string()
    } else {
        let words: Vec<&str> = prompt.split_whitespace().take(4).collect();
        sanitize_story_dirname(&words.join("_"))
    };
    let name = format!("{ts}_{slug}");
    let dir = base.join(&name);
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

    let stories_dir = roco_dir().join("stories");

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

fn parse_positional_prompt(args: &[&str]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if arg.starts_with("--") {
            if !arg.contains('=')
                && matches!(
                    arg,
                    "--strategy" | "--max-tokens" | "--seed" | "--temperature"
                )
            {
                i += 1;
            }
        } else if !arg.starts_with('-') {
            return Some(arg.to_string());
        }
        i += 1;
    }
    None
}

pub fn cmd_story(extra: &[&str]) {
    // Initialize the agent journal so components can log
    let _ = AgentJournal::init();

    let prompt = parse_positional_prompt(extra).unwrap_or_else(|| {
        "Write a short story about a lighthouse keeper who discovers a message in a bottle."
            .to_string()
    });

    let strategy_str = parse_opt("--strategy", extra).unwrap_or("state-tuned");
    let strategy_kind = StrategyKind::parse(strategy_str).unwrap_or(StrategyKind::StateTuned);

    let max_tok_str = parse_opt("--max-tokens", extra).unwrap_or("800");
    let max_tokens = max_tok_str.parse::<usize>().unwrap_or(800);

    let seed_str = parse_opt("--seed", extra);
    let seed = seed_str.and_then(|s| s.parse::<u64>().ok());

    let temp_str = parse_opt("--temperature", extra);
    let temperature = temp_str.and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.7);

    let mock = extra.iter().any(|&a| a == "--mock");
    if mock {
        std::env::set_var("ROCO_USE_MOCK_BACKEND", "1");
        println!("  🎭 Using mock backend (no real model)\n");
    }

    let progress = extra.iter().any(|&a| a == "--progress" || a == "-P");
    if progress {
        std::env::set_var("ROCO_PROGRESS", "1");
    }

    // ── Resume / phase flags ──────────────────────────────────────
    let resume = extra.iter().any(|&a| a == "--resume" || a == "-r");
    let phase_filter = parse_opt("--phase", extra);
    let fix_chapter = if let Some(idx) = extra.iter().position(|&a| a == "--fix") {
        extra.get(idx + 1).and_then(|v| {
            if v.starts_with("chapter") {
                v.strip_prefix("chapter")
                    .and_then(|s| s.trim().parse::<usize>().ok())
            } else {
                v.parse::<usize>().ok()
            }
        })
    } else {
        None
    };
    let workspace_path = parse_opt("--workspace", extra);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    let backend = daemon::ensure_backend();

    let pipeline_result: Result<(), AgentError> = rt.block_on(async move {
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
                temperature,
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

        // Determine workspace: resume existing or create new
        let (ws, existing_outline, existing_wiki, existing_chapters) =
            if let Some(ref wp) = workspace_path {
                let abs_wp = std::path::PathBuf::from(wp).canonicalize().unwrap_or_else(|_| std::path::PathBuf::from(wp));
                let ws = roco_workspace::Workspace::from_existing(
                    abs_wp,
                    WorkspaceKind::Agent
                ).unwrap();
                let outline = read_ws_file(&ws, "01-OUTLINE.md");
                let wiki = read_ws_file(&ws, "02-WIKI.md");
                let chapters = detect_chapters(&ws);
                (ws, outline, wiki, chapters)
            } else if resume || fix_chapter.is_some() {
                let ws = find_latest_workspace().unwrap_or_else(|| {
                    eprintln!("No previous story workspace found. Run without --resume to start fresh.");
                    std::process::exit(1);
                });
                let outline = read_ws_file(&ws, "01-OUTLINE.md");
                let wiki = read_ws_file(&ws, "02-WIKI.md");
                let chapters = detect_chapters(&ws);
                (ws, outline, wiki, chapters)
            } else {
                let ws = create_story_workspace(&prompt).unwrap();
                (ws, None, None, Vec::new())
            };
        let workspace_path = ws.root().to_string_lossy().to_string();

        println!("  Workspace: {workspace_path}\n");

        // Show what we're resuming from
        if existing_outline.is_some() || !existing_chapters.is_empty()
            || phase_filter.is_some() || fix_chapter.is_some()
        {
            println!("  Resuming from existing workspace:");
            if existing_outline.is_some() { println!("    ✓ Outline exists"); }
            if existing_wiki.is_some() { println!("    ✓ Wiki exists"); }
            for &ch_num in &existing_chapters {
                println!("    ✓ Chapter {ch_num} exists");
            }
            if let Some(ref pf) = phase_filter {
                println!("    → Running only phase: {pf}");
            }
            if let Some(ch) = fix_chapter {
                println!("    → Fixing chapter {ch}");
            }
            println!();
        } else {
            println!("  Pipeline: outline → worldbuilding → chapters → validation → synopsis → publish\n");
        }

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

                let outline: StoryOutline = structured_complete_with_strategy(
                    backend,
                    SYSTEM_OUTLINE,
                    &prompt_outline(premise),
                    &outline_strategy_clone,
                    temperature,
                    800,
                    Some(SESSION_WRITER),
                    seed,
                )
                .map_err(|e| AgentError::Internal(format!("outline generation failed: {e}")))?;

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

                Ok(HandlerResult {
                    task: task.clone(),
                    output: format!(
                        "Title: {title}\nGenre: {genre}\nTone: {tone}\nChapters: {}\n",
                        outline.chapters.len()
                    ),
                    files: HashMap::new(),
                    pass: true,
                })
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
                    SYSTEM_WIKI,
                    &prompt_wiki(premise, outline),
                    &wiki_strategy_clone,
                    temperature,
                    1500,
                    Some(SESSION_WRITER),
                    seed,
                )
                .map_err(|e| AgentError::Internal(format!("wiki generation failed: {e}")))?;

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

                Ok(HandlerResult {
                    task: task.clone(),
                    output: md,
                    files: HashMap::new(),
                    pass: true,
                })
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
                let chapter_title = task
                    .spec
                    .get("chapter_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or(chapter_label);
                let chapter_summary = task
                    .spec
                    .get("chapter_summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
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
                let feedback = task
                    .spec
                    .get("feedback")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let label = if is_retry {
                    format!("{chapter_label} (revision)")
                } else {
                    chapter_label.to_string()
                };

                let chapter_temp = if is_retry { (temperature - 0.2).max(0.1) } else { temperature };

                AgentJournal::phase("story", &format!("Writing {label} (phase 3/6)..."));

                let directive = if is_retry {
                    prompt_revision(chapter_label, chapter_title, chapter_summary, &feedback, outline)
                } else {
                    prompt_chapter(chapter_label, chapter_title, chapter_summary, outline, previous, chapter_num == 1)
                };

                // Chapters need more tokens — the model writes 300-500 words of prose
                let chapter_max_tokens = max_tokens.max(1500);

                                let chapter: StoryChapter = structured_complete_with_strategy(
                    backend,
                    SYSTEM_CHAPTER,
                    &directive,
                    &chapter_strategy_clone,
                    chapter_temp,
                    chapter_max_tokens,
                    Some(SESSION_WRITER),
                    seed,
                )
                .map_err(|e| AgentError::Internal(format!("chapter generation failed: {e}")))?;

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

                Ok(HandlerResult {
                    task: task.clone(),
                    output: md,
                    files: HashMap::new(),
                    pass: true,
                })
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
                    Ok(format!("\n## Chapter {chapter_num}\n\n[validation skipped — chapter is empty]\n"))
                } else {
                    structured_complete_with_strategy::<StoryValidation>(
                        backend,
                        SYSTEM_VALIDATOR,
                        &prompt_validation(chapter_text),
                        &val_strategy_clone,
                        0.3,
                        200,
                        Some(SESSION_VALIDATOR),
                        seed,
                    )
                    .map(|v: StoryValidation| {
                        format!(
                            "\n## Chapter {chapter_num}\n\nQuality: {}\nIssues: {}\nSuggestion: {}\n",
                            v.quality, v.issues, v.suggestion
                        )
                    })
                    .map_err(|e| AgentError::Internal(format!("validation failed: {e}")))
                }?;

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

                Ok(HandlerResult {
                    task: task.clone(),
                    output: entry,
                    files: HashMap::new(),
                    pass: true,
                })
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
                    SYSTEM_SYNOPSIS,
                    &prompt_synopsis(chapters),
                    &synopsis_strategy_clone,
                    0.5,
                    400,
                    Some(SESSION_WRITER),
                    seed,
                )
                .map_err(|e| AgentError::Internal(format!("synopsis generation failed: {e}")))?;

                let md = format!("# Synopsis\n\n{}", synopsis.summary);

                let path = ws.resolve("05-SYNOPSIS.md").unwrap();
                let _ = WriteTool.call(json!({"path": path.to_string_lossy(), "content": &md}));

                AgentJournal::action("story", "Synopsis complete");

                Ok(HandlerResult {
                    task: task.clone(),
                    output: md,
                    files: HashMap::new(),
                    pass: true,
                })
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
                    story.push('\n');
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

                Ok(HandlerResult {
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
                })
            }),
        );

        // ═══════════════════════════════════════════════════════════════
        // Execution
        // ═══════════════════════════════════════════════════════════════

        // Phase 1: outline
        let should_run_outline = existing_outline.is_none()
            && phase_filter.as_deref() != Some("wiki")
            && phase_filter.as_deref() != Some("chapter")
            && phase_filter.as_deref() != Some("synopsis")
            && phase_filter.as_deref() != Some("publish")
            && fix_chapter.is_none();
        let outline_text = if let Some(ref existing) = existing_outline {
            println!("📝 Outline (existing)...");
            println!("  ✓ Outline loaded from workspace\n");
            existing.clone()
        } else if should_run_outline {
            println!("📝 Outline...");
            AgentJournal::info("story", "Phase 1: Generating outline");
            let plan = MechPlan {
                tasks: vec![Task {
                    r#type: "compose".into(),
                    domain: "outline".into(),
                    spec: serde_json::json!({"premise": prompt}),
                }],
            };
            let _outline_result = agent
                .dispatch_single(backend.as_ref(), &plan.tasks[0], &ws)?;
            // Read back the markdown file (handler writes full chapter details there)
            let outline_text = read_ws_file(&ws, "01-OUTLINE.md").unwrap_or_default();
            println!("  ✓ Outline complete\n");
            outline_text
        } else {
            println!("📝 Outline (skipped)\n");
            read_ws_file(&ws, "01-OUTLINE.md").unwrap_or_default()
        };

        // Phase 2: wiki
        let should_run_wiki = existing_wiki.is_none()
            && phase_filter.as_deref() != Some("outline")
            && phase_filter.as_deref() != Some("chapter")
            && phase_filter.as_deref() != Some("synopsis")
            && phase_filter.as_deref() != Some("publish")
            && fix_chapter.is_none();
        let _wiki_text: String = if let Some(ref existing) = existing_wiki {
            println!("📚 Worldbuilding (existing)...");
            println!("  ✓ World bible loaded from workspace\n");
            existing.clone()
        } else if should_run_wiki {
            println!("📚 Worldbuilding...");
            AgentJournal::info("story", "Phase 2: Building world bible");
            let wiki_plan = MechPlan {
                tasks: vec![Task {
                    r#type: "compose".into(),
                    domain: "wiki".into(),
                    spec: serde_json::json!({"premise": prompt, "outline": outline_text}),
                }],
            };
            agent.dispatch_single(backend.as_ref(), &wiki_plan.tasks[0], &ws)?;
            println!("  ✓ World bible complete\n");
            read_ws_file(&ws, "02-WIKI.md").unwrap_or_default()
        } else {
            println!("📚 Worldbuilding (skipped)\n");
            read_ws_file(&ws, "02-WIKI.md").unwrap_or_default()
        };

        // Bake instructional state into writer session — primes OUTPUT FORMAT, not content
        // Examples have diverse themes (fantasy, sci-fi) but identical JSON output schema
        println!("  🔥 Baking writer session (format: JSON chapter prose)...");
        AgentJournal::info("story", "Baking writer session with format examples");
        let bake_result = futures::executor::block_on(
            backend.bake_state(
                SESSION_WRITER,
                "You write fiction chapters. You always output JSON with title and content fields. \
                 Never include thinking, reasoning, or meta-commentary. Only the JSON object.",
                BAKE_CHAPTER_EXAMPLES,
            )
        );
        match bake_result {
            Ok(sid) => AgentJournal::action("story", &format!("Writer session baked: {sid}")),
            Err(e) => AgentJournal::warn("story", &format!("Writer session baking skipped: {e}")),
        }

        // Bake instructional state into validator session — primes JSON `{quality, issues, suggestion}` format
        println!("  🔥 Baking validator session (format: JSON quality report)...");
        AgentJournal::info("story", "Baking validator session with format examples");
        let val_bake_result = futures::executor::block_on(
            backend.bake_state(
                SESSION_VALIDATOR,
                "You review fiction chapters. You always output JSON with quality, issues, and suggestion fields. \
                 Never include thinking, reasoning, or meta-commentary. Only the JSON object.",
                BAKE_VALIDATION_EXAMPLES,
            )
        );
        match val_bake_result {
            Ok(sid) => AgentJournal::action("story", &format!("Validator session baked: {sid}")),
            Err(e) => AgentJournal::warn("story", &format!("Validator session baking skipped: {e}")),
        }

        // Phase 3: chapters ×3
        AgentJournal::info("story", "Phase 3: Writing chapters");
        let mut chapter_texts = Vec::new();
        for i in 1..=3 {
            // Check if we should skip this chapter
            let chapter_exists = existing_chapters.contains(&i);
            let should_run_chapter = if chapter_exists && fix_chapter != Some(i) {
                // Chapter exists and we're not fixing it — only run if explicitly requesting chapters
                phase_filter.as_deref() == Some("chapter")
            } else {
                // Chapter doesn't exist or we're fixing it — run unless we're only doing synopsis/publish
                !matches!(phase_filter.as_deref(), Some("synopsis") | Some("publish"))
            };

            // Load existing chapter text
            let existing_ch = if chapter_exists {
                read_ws_file(&ws, &format!("03-CHAPTER_{i}.md"))
            } else {
                None
            };

            // Don't run chapters if user specified a different phase, even if chapters are missing
            let phase_restricted = phase_filter.is_some()
                && phase_filter.as_deref() != Some("chapters")
                && phase_filter.as_deref() != Some("fix");
            if (should_run_chapter || existing_ch.is_none()) && !phase_restricted {
                // Generate new chapter

                let chapter_label = format!("Chapter {i}");
                let previous = chapter_texts.last().cloned().unwrap_or_default();
                let (chapter_title, chapter_summary) = chapter_outline_info(&outline_text, i);

                println!("  ✍️  {}...", &chapter_label);

                let ch_task = Task {
                    r#type: "write".into(),
                    domain: "chapter".into(),
                    spec: serde_json::json!({
                        "number": i,
                        "label": chapter_label,
                        "outline": outline_text,
                        "previous": previous,
                        "chapter_title": chapter_title,
                        "chapter_summary": chapter_summary,
                    }),
                };
                let ch_result = agent
                    .dispatch_single(backend.as_ref(), &ch_task, &ws)?;

                // Retry loop: validate → [if fail] revise → re-validate → repeat
                let max_retries = 3;
                let mut current_text = ch_result.output;
                for attempt in 0..=max_retries {
                    // Validation (always runs at least once for the initial version)
                    println!("  🔍 Validating {}...", &chapter_label);
                    let val_task = Task {
                        r#type: "validate".into(),
                        domain: "chapter".into(),
                        spec: serde_json::json!({
                            "number": i,
                            "text": current_text,
                        }),
                    };
                    let val_result = agent
                        .dispatch_single(backend.as_ref(), &val_task, &ws)?;

                    let val_entry = &val_result.output;
                    let needs_revision = val_entry.contains("Quality: fail")
                        || val_entry.contains("Quality: needs-work");

                    if !needs_revision {
                        println!("  ✓ {chapter_label} quality check passed");
                        // Write final version (revision or original) to chapter_texts
                        chapter_texts.push(current_text.clone());
                        break;
                    }

                    if attempt >= max_retries {
                        println!("  ⚠️  {chapter_label} still fails after {max_retries} retries — accepting latest version");
                        AgentJournal::warn("story", &format!(
                            "{chapter_label} still needs revision after {max_retries} retries, accepting"
                        ));
                        chapter_texts.push(current_text.clone());
                        break;
                    }

                    // Extract feedback from validation output
                    let revision_feedback: String = val_entry
                        .lines()
                        .filter(|l| l.starts_with("Issues:") || l.starts_with("Suggestion:"))
                        .map(|l| l.to_string())
                        .collect::<Vec<_>>()
                        .join("\n");

                    println!("  ⚠️  {} needs revision (attempt {}/{}) — retrying...",
                        &chapter_label, attempt + 1, max_retries);
                    AgentJournal::warn("story", &format!(
                        "{chapter_label} needs revision (attempt {}/{}), retrying...",
                        attempt + 1, max_retries
                    ));

                    let retry_task = Task {
                        r#type: "write".into(),
                        domain: "chapter".into(),
                        spec: serde_json::json!({
                            "number": i,
                            "label": chapter_label,
                            "outline": outline_text,
                            "previous": previous,
                            "retry": true,
                            "feedback": revision_feedback,
                        }),
                    };
                    let retry_result = agent
                        .dispatch_single(backend.as_ref(), &retry_task, &ws)?;
                    current_text = retry_result.output;

                    // Write revision to file (will be overwritten if another revision follows)
                    let filename = format!("03-CHAPTER_{i}.md");
                    let path = ws.resolve(&filename).unwrap();
                    let _ = WriteTool.call(json!({
                        "path": path.to_string_lossy(),
                        "content": &current_text,
                    }));
                }
        } else {
            // Load existing chapter
            let ch_text = existing_ch.unwrap_or_default();
            println!("  ✍️  Chapter {i} (existing)");
            chapter_texts.push(ch_text);
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
        agent.dispatch_single(backend.as_ref(), &synopsis_task, &ws)?;
        println!("  ✓ Synopsis complete\n");

        // Phase 5: publish
        println!("📦 Publishing...");
        AgentJournal::info("story", "Phase 6: Publishing");
        let publish_task = Task {
            r#type: "publish".into(),
            domain: "chapter".into(),
            spec: serde_json::json!({}),
        };
        let _publish_result = agent
            .dispatch_single(backend.as_ref(), &publish_task, &ws)?;

        let outcome = agent
            .commit(
                MechPlan { tasks: vec![] },
                vec![],
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

        Ok(())
    });

    if let Err(e) = pipeline_result {
        eprintln!("❌ Story pipeline failed: {e}");
        AgentJournal::warn("story", &format!("Pipeline failed: {e}"));
    }
}
