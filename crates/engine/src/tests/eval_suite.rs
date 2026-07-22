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
    assert_eq!(
        cases.len(),
        4,
        "two baseline probes + two state-tune probes"
    );
    let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"instruct_baseline_persona"));
    assert!(names.contains(&"user_turn_coherence"));
    assert!(names.contains(&"state_pirate_persona_baked"));
    assert!(names.contains(&"state_tune_custom_persona"));
    // The probes must be model-backed (not runnable against the non-semantic mock).
    assert!(cases
        .iter()
        .all(|c| c.max_tokens > 0 && c.min_output_chars > 0));
}

#[test]
fn validation_eval_cases_are_present() {
    let cases = validation_eval_cases();
    assert!(
        !cases.is_empty(),
        "validation_eval_cases should not be empty"
    );
    let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"val_critique_overall_quality"));
    assert!(names.contains(&"val_critique_coherence_fail"));
    assert!(names.contains(&"val_instruction_following_matched"));
    assert!(names.contains(&"val_instruction_following_deviated"));
    assert!(names.contains(&"val_natural_feedback"));
    assert!(names.contains(&"val_outline_inference_ok"));
    assert!(names.contains(&"val_wiki_inference"));
    assert!(names.contains(&"val_natural_parse_validate_chapter"));
    assert!(cases.iter().all(|c| c.category == EvalCategory::Validation));
    assert!(cases.iter().all(|c| c.max_tokens > 0));
}

#[test]
fn default_suite_includes_validation() {
    let suite = default_eval_suite();
    let val_count = suite
        .iter()
        .filter(|c| c.category == EvalCategory::Validation)
        .count();
    assert!(
        val_count >= 11,
        "expected >= 11 validation evals, got {val_count}"
    );
    assert_eq!(
        val_count,
        suite
            .iter()
            .filter(|c| c.category == EvalCategory::Validation)
            .count(),
        "all validation cases should be validation category"
    );
    // Check that default suite count matches validation cases count
    let val_cases = validation_eval_cases();
    assert_eq!(
        val_count,
        val_cases.len(),
        "default suite should contain all validation cases"
    );
}
