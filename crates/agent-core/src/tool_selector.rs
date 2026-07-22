//! Gradual tool disclosure.
//!
//! Instead of dumping the full tool catalogue into every prompt, disclose
//! only the tools relevant to the current task (and recent context). Relevance
//! is a keyword-overlap score over each tool's name + description, reusing the
//! shared `score_text` ranker from `memory`. A safety net returns every tool
//! when nothing scores above zero, so the agent is never left without tools.
//! Satisfies `goals/message/gradual_tool_disclosure.md`.

use std::cmp::Ordering;

use roco_tools::ToolRegistry;

/// Select tool names relevant to `task` (plus optional `context`), best-first.
///
/// Tools with zero relevance are dropped — unless that would leave none, in
/// which case every tool is returned (safety net). `limit` caps the result
/// (0 = no cap).
pub fn select_relevant(
    registry: &ToolRegistry,
    task: &str,
    context: &str,
    limit: usize,
) -> Vec<String> {
    let mut query = task.to_string();
    if !context.is_empty() {
        query.push(' ');
        query.push_str(context);
    }
    let query_tokens = crate::memory::tokenize(&query);
    if query_tokens.is_empty() {
        return registry.names();
    }

    let mut scored: Vec<(f64, String)> = registry
        .names()
        .into_iter()
        .map(|name| {
            let tool = registry.get(&name).expect("registered tool exists");
            let text = format!("{} {}", name, tool.description());
            let score = crate::memory::score_text(&query_tokens, &text, &[], 0);
            (score, name)
        })
        .collect();

    scored.retain(|(s, _)| *s > 0.0);
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));

    // Safety net: never leave the agent without tools.
    if scored.is_empty() {
        return registry.names();
    }

    let limit = if limit == 0 { scored.len() } else { limit };
    scored.into_iter().take(limit).map(|(_, n)| n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use roco_tools::ToolRegistry;

    fn registry() -> ToolRegistry {
        let mut r = ToolRegistry::new();
        for t in roco_tools::all_tools() {
            r.register(t);
        }
        r
    }

    #[test]
    fn discloses_relevant_tools() {
        let reg = registry();
        let picked = select_relevant(&reg, "read configuration yaml", "", 0);
        assert!(
            picked.contains(&"read".to_string()),
            "read should be disclosed"
        );
        assert!(
            !picked.contains(&"bash".to_string()),
            "bash has no lexical overlap"
        );
        assert!(
            !picked.contains(&"now".to_string()),
            "now has no lexical overlap"
        );
        assert!(picked.len() < reg.len(), "disclosure should narrow the set");
    }

    #[test]
    fn irrelevant_query_falls_back_to_all() {
        let reg = registry();
        let picked = select_relevant(&reg, "   ", "", 0);
        assert_eq!(
            picked.len(),
            reg.len(),
            "empty query -> all tools (safety net)"
        );
    }

    #[test]
    fn limit_caps_results() {
        let reg = registry();
        let picked = select_relevant(&reg, "read configuration yaml", "", 1);
        assert!(picked.len() <= 1);
        assert!(picked.contains(&"read".to_string()));
    }
}
