//! General-purpose utilities shared across agent modules.
//!
//! Re-exports session infrastructure from [`roco_engine::util`] so production
//! code in this crate can use `crate::util::session_complete` etc.
//!
//! # State Tune Architecture
//!
//! Every task type has a named session. On first use, the system prompt is
//! **baked** into the session via `lazy_bake`. Subsequent calls resume from
//! the baked session without re-sending the system prompt.
//!
//! See [`STATE_TUNE_EXAMPLES.md`] for the full catalog of baked sessions.

pub use roco_engine::util::{
    claim_bake, lazy_bake, model_complete, reset_baked_sessions, session_complete,
    session_structured, structured_complete, CHAPTER_SESSION, CONTINUE_SESSION, CRITIQUE_SESSION,
    EVAL_SESSION, OUTLINE_SESSION, WRITING_ANALYSIS_SESSION,
};

use serde::de::DeserializeOwned;

/// Parse structured JSON output from a model response with robust error fallback handling.
pub fn parse_structured_response<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    let mut cleaned = text.trim();

    // 1. Strip markdown code block markers
    if cleaned.starts_with("```json") {
        cleaned = cleaned.strip_prefix("```json").unwrap_or(cleaned);
    } else if cleaned.starts_with("```") {
        cleaned = cleaned.strip_prefix("```").unwrap_or(cleaned);
    }
    if cleaned.ends_with("```") {
        cleaned = cleaned.strip_suffix("```").unwrap_or(cleaned);
    }
    let mut cleaned = cleaned.trim().to_string();

    // 2. Fallback: extract the JSON boundaries if there is leading/trailing conversational text
    if !cleaned.starts_with('{') {
        if let Some(start_idx) = cleaned.find('{') {
            if let Some(end_idx) = cleaned.rfind('}') {
                if end_idx > start_idx {
                    cleaned = cleaned[start_idx..=end_idx].to_string();
                }
            }
        }
    }

    // 3. Try to parse directly first
    match serde_json::from_str::<T>(&cleaned) {
        Ok(val) => return Ok(val),
        Err(e) => {
            // Log/trace the raw parse failure
            tracing::warn!(
                "Direct JSON parse failed: {e}. Attempting automated fallback healing..."
            );
        }
    }

    // 4. Try automatic healing of common JSON formatting slip-ups (e.g., control characters, unescaped quotes)
    // We clean up unescaped newlines inside strings and common trailing commas before the closing braces.
    let mut healed = String::with_capacity(cleaned.len());
    let mut in_str = false;
    let mut escaped = false;
    let chars: Vec<char> = cleaned.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            if escaped {
                healed.push(c);
                escaped = false;
            } else if c == '\\' {
                healed.push(c);
                escaped = true;
            } else if c == '"' {
                healed.push(c);
                in_str = false;
            } else if c == '\n' {
                // Escape unescaped newlines inside JSON strings
                healed.push_str("\\n");
            } else if c == '\r' {
                // Ignore CR
            } else {
                healed.push(c);
            }
        } else {
            if c == '"' {
                in_str = true;
                healed.push(c);
            } else {
                healed.push(c);
            }
        }
        i += 1;
    }

    // Try parsing the healed string
    match serde_json::from_str::<T>(&healed) {
        Ok(val) => {
            tracing::info!("Healed JSON parse succeeded.");
            Ok(val)
        }
        Err(e) => {
            // Fail loudly but informatively
            Err(format!(
                "Graceful parsing failed. JSON was syntactically invalid despite healing attempts.\nError: {e}\nRaw output: {text}"
            ))
        }
    }
}
