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
    session_structured, structured_complete, CHAPTER_SESSION, CONTINUE_SESSION,
    CRITIQUE_SESSION, EVAL_SESSION, OUTLINE_SESSION, WRITING_ANALYSIS_SESSION,
};

use serde::de::DeserializeOwned;

/// Parse structured JSON output from a model response.
pub fn parse_structured_response<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str::<T>(text).map_err(|e| format!("parse error: {e}\nraw: {text}"))
}
