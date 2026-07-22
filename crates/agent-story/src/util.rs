//! General-purpose utilities shared across agent modules.

use serde::de::DeserializeOwned;

/// Parse structured JSON output from a model response.
///
/// This is the canonical way to deserialize LLM output that was constrained
/// by a BNF grammar (see `roco_grammar::schema_to_gbnf`). It assumes the
/// text is already valid JSON — the grammar ensures this. The function
/// centralises the error message format so every call site doesn't repeat
/// the same `map_err` boilerplate.
pub fn parse_structured_response<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str::<T>(text).map_err(|e| format!("parse error: {e}\nraw: {text}"))
}
