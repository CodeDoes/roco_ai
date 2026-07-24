//! General-purpose utilities shared across agent modules.

use roco_engine::{CompletionRequest, ModelBackend};
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

/// canonical helper function to perform structured completion with BNF grammar constraints
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

    parse_structured_response(&text)
}
