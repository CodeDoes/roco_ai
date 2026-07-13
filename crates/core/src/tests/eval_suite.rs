//! Tests for `crate::eval_suite`.
//!
//! Included from `eval_suite.rs` via `#[path = "tests/eval_suite.rs"]` so this
//! file is treated as part of the lib crate's `tests` module and runs
//! in the same `cargo test` invocation as the other unit tests.

use super::*;

#[test]
fn grammar_eval_cases_have_compilable_grammars() {
    // Sanity check: grammar strings must parse as GBNF. We pull
    // schoolmarm only when the feature is on so the lib stays
    // buildable with default features.
    #[cfg(feature = "grammar-rwkv")]
    {
        use schoolmarm::Grammar;
        for case in grammar_eval_cases() {
            let g = case
                .grammar
                .as_ref()
                .expect("grammar_eval_cases pin a grammar");
            Grammar::new(g).unwrap_or_else(|e| {
                panic!("grammar in eval case '{}' did not parse: {e:?}", case.name)
            });
        }
    }
    // Without the feature, we just assert the static length.
    #[cfg(not(feature = "grammar-rwkv"))]
    assert!(grammar_eval_cases().iter().all(|c| c.grammar.is_some()));
}

#[cfg(feature = "grammar-rwkv")]
#[test]
fn jsonschema_eval_cases_grammars_parse_through_schoolmarm() {
    // The whole reason jsonschema_eval_cases exists is to ensure
    // JSON-Schema -> GBNF -> schoolmarm::Grammar is a closed
    // chain. Verify it directly by running schoolmarm::Grammar::new
    // on each grammar the eval cases carry.
    use schoolmarm::Grammar;
    for case in jsonschema_eval_cases() {
        let g = case
            .grammar
            .as_ref()
            .expect("jsonschema_eval_cases pin a grammar");
        Grammar::new(g).unwrap_or_else(|e| {
            panic!("grammar in eval case '{}' did not parse: {e:?}", case.name)
        });
    }
}
