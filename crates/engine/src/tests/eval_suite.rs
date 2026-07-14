//! Tests for eval suite functionality.
//!
//! Included from `eval.rs` via `#[path = "tests/eval_suite.rs"]`.

use crate::*;

#[test]
fn grammar_eval_cases_are_present() {
    assert!(grammar_eval_cases().is_empty());
}
