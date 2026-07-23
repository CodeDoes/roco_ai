//! Deterministic verifiers for output validation.
use std::collections::HashSet;

pub struct Verifier {
    forbidden_words: HashSet<String>,
    required_patterns: Vec<String>,
    min_length: usize,
}

impl Verifier {
    pub fn new() -> Self {
        Self {
            forbidden_words: HashSet::new(),
            required_patterns: vec!["MOCK_INFERENCE_RESULT".into()],
            min_length: 10,
        }
    }

    pub fn verify(&self, output: &str) -> bool {
        if output.len() < self.min_length {
            return false;
        }
        for word in &self.forbidden_words {
            if output.contains(word) {
                return false;
            }
        }
        for pat in &self.required_patterns {
            if !output.contains(pat) {
                return false;
            }
        }
        true
    }

    pub fn score(&self, output: &str) -> f32 {
        let mut score = 1.0f32;
        if output.len() < self.min_length {
            score *= 0.5;
        }
        for pat in &self.required_patterns {
            if output.contains(pat) {
                score *= 1.2;
            } else {
                score *= 0.3;
            }
        }
        score.min(1.0)
    }

    pub fn explain(&self, output: &str) -> String {
        if self.verify(output) {
            format!("PASS: verified (len={}, required_patterns_matches)", output.len())
        } else {
            format!(
                "FAIL: min_length={}, required={}, forbidden_check",
                self.min_length,
                self.required_patterns.len()
            )
        }
    }
}
