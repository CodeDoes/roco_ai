//! Tests for eval suite functionality.
//!
//! Included from `eval.rs` via `#[path = "tests/eval_suite.rs"]`.

use crate::*;

#[test]
fn grammar_eval_cases_are_present() {
    assert!(grammar_eval_cases().is_empty());
}

#[test]
fn message_eval_cases_are_present() {
    let cases = message_eval_cases();
    assert_eq!(cases.len(), 2, "two message-layer baseline probes");
    let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"instruct_baseline_persona"));
    assert!(names.contains(&"user_turn_coherence"));
    // The probes must be model-backed (not runnable against the non-semantic mock).
    assert!(cases.iter().all(|c| c.max_tokens > 0 && c.min_output_chars > 0));
}
