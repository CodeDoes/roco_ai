//! Utility functions for text processing used by evals and other engine modules.

/// Clean story text by stripping thinking blocks and fixing paragraph separation.
pub fn clean_story_text(text: &str) -> String {
    let text = strip_thinking(text);
    let text = fix_paragraphs(&text);
    text.trim().to_string()
}

/// Strip ` thinking...  ` and similar reasoning blocks from model output.
fn strip_thinking(text: &str) -> String {
    let mut result = String::new();
    let mut in_think = false;
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();

    while i < chars.len() {
        if i + 10 <= chars.len() {
            let window: String = chars[i..i + 10].iter().collect();
            if window.starts_with(" thinking") || window.starts_with(" \u{1f50d}") {
                in_think = true;
                i += 10;
                continue;
            }
        }
        if in_think {
            // Look for closing tag
            if i + 3 <= chars.len() {
                let close: String = chars[i..i + 3].iter().collect();
                if close == " response" || close == " \u{2728}" || close == " \u{2705}" {
                    in_think = false;
                    i += 3;
                    continue;
                }
            }
            if chars[i] == '\n' {
                // If we see a newline while in think mode and the next line
                // doesn't look like continuing thinking, end the block
                in_think = false;
                result.push('\n');
            }
            // Skip thinking character
            i += 1;
            continue;
        }
        // Check for closing tag when not in think mode (stray)
        if i + 9 <= chars.len() {
            let stray: String = chars[i..i + 9].iter().collect();
            if stray.starts_with(" response") {
                // Skip the closing tag (it's a stray)
                i += 9;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Fix paragraphs: ensure paragraphs are separated by double newlines.
fn fix_paragraphs(text: &str) -> String {
    let mut result = String::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if line.trim().is_empty() {
            result.push_str("\n\n");
            i += 1;
            continue;
        }

        // Check if this line is the end of a paragraph
        let is_para_break = if i + 1 < lines.len() {
            let next = lines[i + 1].trim();
            if next.is_empty() {
                false
            } else if line.ends_with('.')
                || line.ends_with('!')
                || line.ends_with('?')
                || line.ends_with('"')
                || line.ends_with('"')
                || line.ends_with('—')
            {
                next.starts_with(|c: char| c.is_uppercase() || c == '"' || c == '*' || c == '#')
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

    // Clean up multiple blank lines
    let mut cleaned = String::new();
    let mut prev_blank = false;
    for line in result.lines() {
        if line.trim().is_empty() {
            if !prev_blank {
                cleaned.push_str("\n\n");
                prev_blank = true;
            }
        } else {
            cleaned.push_str(line);
            cleaned.push('\n');
            prev_blank = false;
        }
    }

    let cleaned = cleaned.trim().to_string();
    cleaned.replace("\n\n\n", "\n\n")
}

// ═════════════════════════════════════════════════════════════════════════════
// State Tune Session Infrastructure
// ═════════════════════════════════════════════════════════════════════════════
//
// Named sessions for state-tuned model calls. Each session corresponds to a
// task type (outline, chapter, critique, etc.). On first use, the system
// prompt + few-shot examples are baked into the session via
// `bake_no_think_session`. Subsequent calls resume from the baked session
// without re-sending the system prompt.
//
// See [`STATE_TUNE_EXAMPLES.md`](https://github.com/roco-ai/roco/blob/main/STATE_TUNE_EXAMPLES.md)
// for the full catalog of baked sessions.

use crate::backend::bake_no_think_session;
use crate::types::CompletionRequest;
use crate::ModelBackend;
use serde::de::DeserializeOwned;

// ═════════════════════════════════════════════════════════════════════════════
// Named sessions — one per task type
// ═════════════════════════════════════════════════════════════════════════════

pub const OUTLINE_SESSION: &str = "roco_outline";
pub const CHAPTER_SESSION: &str = "roco_chapter";
pub const CONTINUE_SESSION: &str = "roco_continue";
pub const CRITIQUE_SESSION: &str = "roco_critique";
pub const EVAL_SESSION: &str = "roco_eval";
pub const WRITING_ANALYSIS_SESSION: &str = "roco_writing_analysis";
pub const FIM_SESSION: &str = "fim_session";
pub const CHAT_SESSION: &str = "roco_chat";
pub const INTENT_SESSION: &str = "roco_intent";

// ═════════════════════════════════════════════════════════════════════════════
// Baked session tracker — forces each session to be baked at most once per
// process lifetime. Uses a global static set so lazy bakes are thread-safe.
// ═════════════════════════════════════════════════════════════════════════════

use std::sync::LazyLock;
use std::sync::Mutex;

static BAKED_SESSIONS: LazyLock<Mutex<std::collections::HashSet<&'static str>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

/// Check (and atomically claim) whether a session has been baked in this
/// process lifetime. Returns `true` if this is the **first** claim.
pub fn claim_bake(session: &'static str) -> bool {
    BAKED_SESSIONS.lock().unwrap().insert(session)
}

/// Reset all baked-session flags (used in tests or after a backend reload).
pub fn reset_baked_sessions() {
    BAKED_SESSIONS.lock().unwrap().clear();
}

// ═════════════════════════════════════════════════════════════════════════════
// Lazy bake — call this before the first session-based generation
// ═════════════════════════════════════════════════════════════════════════════

/// Bake a system prompt + few-shot examples into a named session, but only
/// on the first call (tracked by `BAKED_SESSIONS`). Subsequent calls are
/// no-ops. Must be called from a tokio runtime context.
pub fn lazy_bake(
    backend: &dyn ModelBackend,
    session: &'static str,
    system: &str,
    examples: &[(&str, &str)],
) -> Result<(), String> {
    if claim_bake(session) {
        futures::executor::block_on(bake_no_think_session(backend, session, system, examples))
            .map_err(|e| format!("state-tune bake failed for {session}: {e}"))
    } else {
        Ok(())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Session-based generation — resume from a baked session
// ═════════════════════════════════════════════════════════════════════════════

/// Generate text from a baked session. The system prompt was absorbed during
/// `lazy_bake`; only `prompt` is sent.
pub fn session_complete(
    backend: &dyn ModelBackend,
    session: &str,
    prompt: &str,
    grammar: Option<&str>,
    temperature: f32,
    max_tokens: usize,
    prefill: Option<&str>,
) -> Result<String, String> {
    futures::executor::block_on(backend.complete(CompletionRequest {
        system: String::new(),
        prompt: prompt.to_string(),
        grammar: grammar.map(String::from),
        temperature,
        max_tokens,
        prefill: prefill.map(String::from),
        session: Some(session.to_string()),
        preserve_state: true,
        ..Default::default()
    }))
    .map_err(|e| format!("model error in session {session}: {e}"))
    .map(|r| r.text)
}

/// Like `session_complete` but deserializes the response as JSON.
pub fn session_structured<T>(
    backend: &dyn ModelBackend,
    session: &str,
    prompt: &str,
    grammar: &str,
    temperature: f32,
    max_tokens: usize,
) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let text = session_complete(
        backend,
        session,
        prompt,
        Some(grammar),
        temperature,
        max_tokens,
        None,
    )?;
    serde_json::from_str::<T>(&text).map_err(|e| format!("parse error: {e}\nraw: {text}"))
}

// ═════════════════════════════════════════════════════════════════════════════
// Direct (no-session) helpers — for one-off calls without baking
// ═════════════════════════════════════════════════════════════════════════════

/// One-off model call with system + prompt + grammar. No session.
pub fn model_complete(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    grammar: Option<&str>,
    temperature: f32,
    max_tokens: usize,
    prefill: Option<&str>,
) -> Result<String, String> {
    futures::executor::block_on(backend.complete(CompletionRequest {
        system: system.to_string(),
        prompt: prompt.to_string(),
        grammar: grammar.map(String::from),
        temperature,
        max_tokens,
        prefill: prefill.map(String::from),
        ..Default::default()
    }))
    .map_err(|e| format!("model error: {e}"))
    .map(|r| r.text)
}

/// One-off model call with JSON deserialization. No session.
pub fn structured_complete<T>(
    backend: &dyn ModelBackend,
    system: &str,
    prompt: &str,
    grammar: &str,
    temperature: f32,
    max_tokens: usize,
) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let text = model_complete(
        backend,
        system,
        prompt,
        Some(grammar),
        temperature,
        max_tokens,
        None,
    )?;
    serde_json::from_str::<T>(&text).map_err(|e| format!("parse error: {e}\nraw: {text}"))
}
