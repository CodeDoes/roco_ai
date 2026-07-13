//! Standalone model evaluation framework.
//!
//! Tests a [`ModelBackend`] directly (not through the orchestrator pipeline)
//! on concrete capabilities: instruction following, output coherence,
//! repetition avoidance, throughput, etc. Produces structured JSON reports.
//!
//! ```bash
//! # Run against the Mock backend (no model needed, quick smoke test)
//! cargo run --example eval_suite
//!
//! # Run against the local RWKV model
//! cargo run --example eval_suite --release -- --backend rwkv
//!
//! # Run a specific eval
//! cargo run --example eval_suite -- --filter coherence
//! ```

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::engine::{CompletionRequest, ModelBackend, TokenUsage};

// ---------------------------------------------------------------------------
// Eval case definition
// ---------------------------------------------------------------------------

/// A single evaluation test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    /// Unique name for this eval case (used for filtering).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// System prompt.
    pub system: String,
    /// User prompt to send.
    pub prompt: String,
    /// Expected behavior hints — strings we check for presence in output.
    pub expected_hints: Vec<String>,
    /// Strings that must NOT appear in output (repetition, gibberish markers).
    pub forbidden_strings: Vec<String>,
    /// Maximum number of tokens to generate.
    pub max_tokens: usize,
    /// Sampling temperature.
    pub temperature: f32,
    /// Minimum acceptable output length in characters.
    pub min_output_chars: usize,
    /// Optional GBNF grammar. Forwarded to the backend via
    /// `CompletionRequest::grammar`. Backends that support grammar-constrained
    /// decoding (e.g. RWKV with the `grammar-rwkv` feature) will mask logits
    /// to keep every sampled token in lockstep with the grammar.
    #[serde(default)]
    pub grammar: Option<String>,
    /// Category for grouping in reports.
    pub category: EvalCategory,
}

/// Categories of evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalCategory {
    /// Quick smoke test: does the backend respond at all?
    Smoke,
    /// Instruction following: does the model do what it's told?
    Instruction,
    /// Output coherence: is the text sensible, grammatical, on-topic?
    Coherence,
    /// Repetition detection: does the model loop or repeat itself?
    Repetition,
    /// Throughput: tokens per second.
    Throughput,
    /// Output format compliance: JSON, tool calls, etc.
    Format,
    /// Context window: can the model handle long inputs?
    Context,
}

impl std::fmt::Display for EvalCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Self::Smoke => "smoke",
            Self::Instruction => "instruction",
            Self::Coherence => "coherence",
            Self::Repetition => "repetition",
            Self::Throughput => "throughput",
            Self::Format => "format",
            Self::Context => "context",
        })
    }
}

// ---------------------------------------------------------------------------
// Check results
// ---------------------------------------------------------------------------

/// Result of a single check within an eval case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Result of running one eval case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub name: String,
    pub description: String,
    pub category: EvalCategory,
    pub passed: bool,
    pub output: String,
    pub latency_ms: u64,
    pub token_usage: TokenUsage,
    /// Tokens per second (completion tokens / wall time).
    pub tokens_per_sec: f64,
    pub checks: Vec<CheckResult>,
    pub errors: Vec<String>,
}

/// Full report from running a suite of evals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub suite_name: String,
    pub backend_name: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub total_latency_ms: u64,
    pub results: Vec<EvalResult>,
    pub category_breakdown: Vec<CategoryBreakdown>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryBreakdown {
    pub category: String,
    pub total: usize,
    pub passed: usize,
}

impl EvalReport {
    pub fn summary(&self) -> String {
        let pct = if self.total > 0 {
            (self.passed as f64 / self.total as f64) * 100.0
        } else {
            0.0
        };
        format!(
            "Eval suite '{suite}': {passed}/{total} passed ({pct:.0}%) on backend '{backend}' in {ms}ms",
            suite = self.suite_name,
            passed = self.passed,
            total = self.total,
            pct = pct,
            backend = self.backend_name,
            ms = self.total_latency_ms,
        )
    }
}

// ---------------------------------------------------------------------------
// Running evals
// ---------------------------------------------------------------------------

/// Run a single eval case against a backend.
pub async fn run_eval<B: ModelBackend + Send + Sync>(
    backend: &B,
    case: &EvalCase,
) -> EvalResult {
    let mut errors: Vec<String> = Vec::new();
    let mut checks: Vec<CheckResult> = Vec::new();

    let request = CompletionRequest {
        system: case.system.clone(),
        prompt: case.prompt.clone(),
        output_schema: None,
        grammar: case.grammar.clone(),
        temperature: case.temperature,
        max_tokens: case.max_tokens,
        estimated_prompt_tokens: 0,
    };

    let start = Instant::now();
    let response = backend.complete(request).await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match response {
        Ok(resp) => {
            let output = resp.text;
            let usage = resp.usage;
            let tokens_per_sec = if usage.completion_tokens > 0 && latency_ms > 0 {
                (usage.completion_tokens as f64 / latency_ms as f64) * 1000.0
            } else {
                0.0
            };

            // --- Checks --- //

            // 1. Non-empty output
            let non_empty = !output.trim().is_empty();
            checks.push(CheckResult {
                name: "non_empty".into(),
                passed: non_empty,
                detail: if non_empty {
                    format!("output length: {} chars", output.len())
                } else {
                    "output was empty".into()
                },
            });

            // 2. Min output length
            let min_len_ok = output.len() >= case.min_output_chars;
            checks.push(CheckResult {
                name: "min_output_length".into(),
                passed: min_len_ok,
                detail: if min_len_ok {
                    format!("{} >= {} chars", output.len(), case.min_output_chars)
                } else {
                    format!("{} < {} chars", output.len(), case.min_output_chars)
                },
            });

            // 3. Expected hints present
            for hint in &case.expected_hints {
                let found = output.to_lowercase().contains(&hint.to_lowercase());
                checks.push(CheckResult {
                    name: format!("hint: {hint}"),
                    passed: found,
                    detail: if found {
                        format!("found '{hint}' in output")
                    } else {
                        format!("expected '{hint}' not found in output")
                    },
                });
            }

            // 4. Forbidden strings absent
            for bad in &case.forbidden_strings {
                let found = output.contains(bad);
                checks.push(CheckResult {
                    name: format!("forbidden: {bad}"),
                    passed: !found,
                    detail: if found {
                        format!("found forbidden string '{bad}' in output")
                    } else {
                        format!("'{bad}' not found (good)")
                    },
                });
            }

            // 5. Repetition check (simple: repeated sentence detection)
            let sentences: Vec<&str> = output
                .split(|c: char| c == '.' || c == '!' || c == '?')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let repeats = if sentences.len() >= 4 {
                let unique: std::collections::HashSet<&str> = sentences.iter().copied().collect();
                sentences.len() - unique.len()
            } else {
                0
            };
            checks.push(CheckResult {
                name: "repetition_check".into(),
                passed: repeats <= 1,
                detail: if repeats <= 1 {
                    format!("{} repeated sentences out of {}", repeats, sentences.len())
                } else {
                    format!("{} repeated sentences out of {} — may be looping", repeats, sentences.len())
                },
            });

            // 6. Throughput check (informational — only fails if latency > 0 but throughput is zero)
            if usage.completion_tokens >= 10 {
                let tp_ok = if latency_ms == 0 { true } else { tokens_per_sec >= 1.0 };
                checks.push(CheckResult {
                    name: "throughput".into(),
                    passed: tp_ok,
                    detail: format!("{:.1} tok/s ({} tokens in {}ms)", tokens_per_sec, usage.completion_tokens, latency_ms),
                });
            }

            let passed = checks.iter().all(|c| c.passed);
            info!(
                eval = case.name,
                passed,
                latency_ms,
                tokens = usage.completion_tokens,
                "eval result"
            );

            EvalResult {
                name: case.name.clone(),
                description: case.description.clone(),
                category: case.category,
                passed,
                output,
                latency_ms,
                token_usage: usage,
                tokens_per_sec,
                checks,
                errors,
            }
        }
        Err(e) => {
            errors.push(format!("{e}"));
            info!(eval = case.name, error = %e, "eval failed with error");
            EvalResult {
                name: case.name.clone(),
                description: case.description.clone(),
                category: case.category,
                passed: false,
                output: String::new(),
                latency_ms,
                token_usage: TokenUsage::default(),
                tokens_per_sec: 0.0,
                checks,
                errors,
            }
        }
    }
}

/// Run a suite of eval cases against a backend.
pub async fn run_suite<B: ModelBackend + Send + Sync>(
    suite_name: &str,
    backend: &B,
    cases: &[EvalCase],
    filter: Option<&str>,
) -> EvalReport {
    let mut results = Vec::new();
    let start = Instant::now();

    for case in cases {
        if let Some(filter) = filter {
            if !case.name.contains(filter) && !case.description.contains(filter) && case.category.to_string() != filter {
                continue;
            }
        }
        let result = run_eval(backend, case).await;
        results.push(result);
    }

    let total_latency_ms = start.elapsed().as_millis() as u64;
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;

    // Category breakdown
    let mut cat_map: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
    for r in &results {
        let cat = r.category.to_string();
        let entry = cat_map.entry(cat).or_insert((0, 0));
        entry.0 += 1;
        if r.passed {
            entry.1 += 1;
        }
    }
    let category_breakdown: Vec<CategoryBreakdown> = cat_map
        .into_iter()
        .map(|(category, (total, passed))| CategoryBreakdown { category, total, passed })
        .collect();

    EvalReport {
        suite_name: suite_name.to_string(),
        backend_name: backend.name().to_string(),
        total,
        passed,
        failed,
        total_latency_ms,
        results,
        category_breakdown,
    }
}

// ---------------------------------------------------------------------------
// Built-in eval cases
// ---------------------------------------------------------------------------

/// Default set of eval cases covering smoke, instruction, coherence, repetition,
/// and format categories.
pub fn default_eval_suite() -> Vec<EvalCase> {
    vec![
        // --- Smoke --- //
        EvalCase {
            name: "smoke_basic_reply".into(),
            description: "Basic smoke test: model responds to a simple prompt".into(),
            system: "You are a helpful assistant.".into(),
            prompt: "Say the word 'hello' and nothing else.".into(),
            expected_hints: vec!["hello".into()],
            forbidden_strings: vec![],
            max_tokens: 50,
            temperature: 0.1,
            min_output_chars: 1,
            grammar: None,
            category: EvalCategory::Smoke,
        },
        EvalCase {
            name: "smoke_empty_system".into(),
            description: "Smoke test with empty system prompt".into(),
            system: "".into(),
            prompt: "Respond with the number 42.".into(),
            expected_hints: vec!["42".into()],
            forbidden_strings: vec![],
            max_tokens: 30,
            temperature: 0.1,
            min_output_chars: 1,
            grammar: None,
            category: EvalCategory::Smoke,
        },

        // --- Instruction Following --- //
        EvalCase {
            name: "instruct_format_constraint".into(),
            description: "Model follows a strict output format instruction".into(),
            system: "You always output JSON.".into(),
            prompt: "List three colors in JSON format like this: {\"colors\": [\"red\", \"green\", \"blue\"]}".into(),
            expected_hints: vec!["colors".into(), "[".into(), "]".into()],
            forbidden_strings: vec![],
            max_tokens: 100,
            temperature: 0.2,
            min_output_chars: 20,
            grammar: None,
            category: EvalCategory::Instruction,
        },
        EvalCase {
            name: "instruct_step_by_step".into(),
            description: "Model follows multi-step instruction".into(),
            system: "You are a precise assistant.".into(),
            prompt: "Follow these steps exactly:\n1. Say 'Step 1 complete'\n2. Say 'Step 2 complete'\n3. Say 'All steps done'".into(),
            expected_hints: vec!["Step 1".into(), "Step 2".into(), "All steps".into()],
            forbidden_strings: vec![],
            max_tokens: 100,
            temperature: 0.2,
            min_output_chars: 30,
            grammar: None,
            category: EvalCategory::Instruction,
        },
        EvalCase {
            name: "instruct_negative".into(),
            description: "Model follows a negative instruction (what NOT to do)".into(),
            system: "You are a helpful assistant.".into(),
            prompt: "Tell me about the weather, but do NOT mention rain, snow, or temperature.".into(),
            expected_hints: vec!["weather".into()],
            forbidden_strings: vec!["rain".into(), "snow".into(), "temperature".into()],
            max_tokens: 100,
            temperature: 0.3,
            min_output_chars: 30,
            grammar: None,
            category: EvalCategory::Instruction,
        },

        // --- Coherence --- //
        EvalCase {
            name: "coherence_explain".into(),
            description: "Model produces coherent explanation of a simple concept".into(),
            system: "You are a teacher.".into(),
            prompt: "Explain what a variable is in programming in one paragraph.".into(),
            expected_hints: vec!["variable".into(), "value".into(), "store".into()],
            forbidden_strings: vec![],
            max_tokens: 150,
            temperature: 0.3,
            min_output_chars: 50,
            grammar: None,
            category: EvalCategory::Coherence,
        },
        EvalCase {
            name: "coherence_story".into(),
            description: "Model tells a short coherent story".into(),
            system: "You are a storyteller.".into(),
            prompt: "Write a 3-sentence story about a robot learning to paint.".into(),
            expected_hints: vec!["robot".into(), "paint".into()],
            forbidden_strings: vec![],
            max_tokens: 150,
            temperature: 0.5,
            min_output_chars: 40,
            grammar: None,
            category: EvalCategory::Coherence,
        },

        // --- Repetition --- //
        EvalCase {
            name: "repetition_avoidance".into(),
            description: "Model avoids repeating the same phrase multiple times".into(),
            system: "You are a helpful assistant.".into(),
            prompt: "List 5 different animals. Write each on a new line.".into(),
            expected_hints: vec![],
            forbidden_strings: vec![],
            max_tokens: 100,
            temperature: 0.5,
            min_output_chars: 20,
            grammar: None,
            category: EvalCategory::Repetition,
        },

        // --- Format --- //
        EvalCase {
            name: "format_json".into(),
            description: "Model outputs valid JSON when asked".into(),
            system: "You are a data formatter. Always output valid JSON.".into(),
            prompt: "Output a JSON object with keys: name, age, city. Use example values.".into(),
            expected_hints: vec!["\"name\"".into(), "\"age\"".into(), "\"city\"".into()],
            forbidden_strings: vec![],
            max_tokens: 100,
            temperature: 0.1,
            min_output_chars: 20,
            grammar: None,
            category: EvalCategory::Format,
        },
        EvalCase {
            name: "format_list".into(),
            description: "Model outputs a numbered list when asked".into(),
            system: "You are a list maker.".into(),
            prompt: "List 3 things you need for a picnic, numbered 1 to 3.".into(),
            expected_hints: vec!["1.".into(), "2.".into(), "3.".into()],
            forbidden_strings: vec![],
            max_tokens: 100,
            temperature: 0.3,
            min_output_chars: 30,
            grammar: None,
            category: EvalCategory::Format,
        },
    ]
}

/// Throughput-specific eval cases (generate many tokens to measure speed).
pub fn throughput_eval_cases() -> Vec<EvalCase> {
    vec![EvalCase {
        name: "throughput_long_gen".into(),
        description: "Generate a substantial amount of text to measure tokens/second".into(),
        system: "You are a creative writer.".into(),
        prompt: "Write a detailed paragraph about the future of artificial intelligence, including its potential benefits and risks. Write at least 200 words.".into(),
        expected_hints: vec![],
        forbidden_strings: vec![],
        max_tokens: 512,
        temperature: 0.4,
        min_output_chars: 100,
        grammar: None,
        category: EvalCategory::Throughput,
    }]
}

/// Context window eval cases (long input prompts).
pub fn context_eval_cases(long_text: &str) -> Vec<EvalCase> {
    vec![EvalCase {
        name: "context_long_input".into(),
        description: "Model handles a long input prompt and answers correctly about it".into(),
        system: "You are a precise reader. Answer questions about the text.".into(),
        prompt: format!("Read this text and then answer: what is the main topic?\n\n{}", long_text),
        expected_hints: vec![],
        forbidden_strings: vec!["I don't know".into(), "I cannot".into(), "I'm not sure".into()],
        max_tokens: 200,
        temperature: 0.2,
        min_output_chars: 20,
        grammar: None,
        category: EvalCategory::Context,
    }]
}

/// Grammar-constrained eval cases.
///
/// Each case pins a hand-written GBNF grammar into `EvalCase::grammar`.
/// The rwkv_backend (with the `grammar-rwkv` feature on) compiles
/// the grammar once per call via schoolmarm and masks logits at every
/// sample step. The expected output is *guaranteed* to be in the
/// grammar's language — not by post-hoc verification, by construction.
///
/// These don't need a JSON-Schema->GBNF converter to ship; the
/// grammars are kept intentionally short so a contributor reading
/// the case can hand-edit them. Once a converter ships, replace
/// these strings with `jsonschema_to_gbnf::schema_to_gbnf(...)` and
/// add proper `expected_hints` matching the JSON contract.
pub fn grammar_eval_cases() -> Vec<EvalCase> {
    // Tiny yes/no grammar is the most common raw-test for schoolmarm:
    // breadth-1 alternative, no whitespace, no recursion.
    let yes_no: &str = r#"root ::= "yes" | "no""#;

    // Integer-1-9 grammar: 9-branch alternative, validates the
    // walker handles width correctly.
    let digit: &str = r#"root ::= "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9""#;

    // Bracket-delimited literal: tests deep acceptance through
    // bounded alternation depth.
    let paren_lit: &str = r#"root ::= "(" [a-z]+ ")""#;

    vec![
        EvalCase {
            name: "grammar_yes_no".into(),
            description: "Grammar-constrained yes/no response. Pins 'root ::= \"yes\" | \"no\"'.".into(),
            system: "You are a precise assistant.".into(),
            prompt: "Are you a helpful model? Answer yes or no.".into(),
            expected_hints: vec![],
            // We expect the model to emit one of {"yes","no"}; both
            // pass without post-hoc checks because backend enforces it.
            forbidden_strings: vec![],
            max_tokens: 8,
            temperature: 0.5,
            min_output_chars: 2,
            grammar: Some(yes_no.to_string()),
            category: EvalCategory::Format,
        },
        EvalCase {
            name: "grammar_digit_1_to_9".into(),
            description: "Grammar-constrained single-digit response. Pins a 9-branch alternative.".into(),
            system: "".into(),
            prompt: "Pick a digit from one to nine.".into(),
            expected_hints: vec![],
            forbidden_strings: vec![],
            max_tokens: 4,
            temperature: 0.5,
            min_output_chars: 1,
            grammar: Some(digit.to_string()),
            category: EvalCategory::Format,
        },
        EvalCase {
            name: "grammar_parens_literal".into(),
            description: "Grammar-constrained (foo)-style response.".into(),
            system: "".into(),
            prompt: "Write exactly one word in parentheses.".into(),
            expected_hints: vec![],
            forbidden_strings: vec![],
            max_tokens: 8,
            temperature: 0.5,
            min_output_chars: 5,
            grammar: Some(paren_lit.to_string()),
            category: EvalCategory::Format,
        },
    ]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a report to a JSON file.
pub fn write_report(path: impl AsRef<std::path::Path>, report: &EvalReport) -> Result<(), std::io::Error> {
    let json = serde_json::to_string_pretty(report)?;
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)
}

/// Print a human-readable summary of a report.
pub fn print_report(report: &EvalReport) {
    println!();
    println!("═══════════════════════════════════════════════════");
    println!("  Eval Report: {}", report.suite_name);
    println!("  Backend:      {}", report.backend_name);
    println!("  Results:      {}/{} passed ({:.0}%)", report.passed, report.total,
        if report.total > 0 { report.passed as f64 / report.total as f64 * 100.0 } else { 0.0 });
    println!("  Total time:   {}ms", report.total_latency_ms);
    println!("═══════════════════════════════════════════════════");

    if !report.category_breakdown.is_empty() {
        println!();
        println!("  Category Breakdown:");
        for cb in &report.category_breakdown {
            println!("    {:>12}: {}/{}", cb.category, cb.passed, cb.total);
        }
    }

    println!();
    for result in &report.results {
        let symbol = if result.passed { "✅" } else { "❌" };
        println!("  {} {} ({})", symbol, result.name, result.category);
        if !result.passed {
            for check in &result.checks {
                if !check.passed {
                    println!("         ↳ {}: {}", check.name, check.detail);
                }
            }
            for err in &result.errors {
                println!("         ↳ error: {}", err);
            }
        }
        if result.latency_ms > 0 {
            print!("         latency: {}ms", result.latency_ms);
            if result.token_usage.completion_tokens > 0 {
                print!(", {} tok/s ({}+{} tokens)",
                    result.tokens_per_sec.round(),
                    result.token_usage.prompt_tokens,
                    result.token_usage.completion_tokens);
            }
            println!();
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
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
                let g = case.grammar.as_ref().expect("grammar_eval_cases pin a grammar");
                Grammar::new(g).unwrap_or_else(|e| {
                    panic!("grammar in eval case ‘{}’ did not parse: {e:?}", case.name)
                });
            }
        }
        // Without the feature, we just assert the static length.
        #[cfg(not(feature = "grammar-rwkv"))]
        assert!(grammar_eval_cases().iter().all(|c| c.grammar.is_some()));
    }
}

