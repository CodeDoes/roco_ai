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
    /// Expected ideal output for comparison ("oracle"). When set, the trace
    /// shows the oracle alongside the actual output so you can judge quality
    /// at a glance. The automated checks still use `expected_hints` for pass/
    /// fail.
    #[serde(default)]
    pub oracle: Option<String>,
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
    /// The prompt that was sent to the model (with role prefixes).
    pub input: String,
    pub output: String,
    pub latency_ms: u64,
    pub token_usage: TokenUsage,
    /// Tokens per second (completion tokens / wall time).
    pub tokens_per_sec: f64,
    pub checks: Vec<CheckResult>,
    pub errors: Vec<String>,
    /// Oracle text this eval's output was compared against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle: Option<String>,
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
///
/// If `trace_path` is `Some`, the full input is written to the file before
/// inference starts, and every generated token is appended as it arrives.
pub async fn run_eval<B: ModelBackend + Send + Sync>(
    backend: &B,
    case: &EvalCase,
    trace_path: Option<&std::path::Path>,
) -> EvalResult {
    let mut errors: Vec<String> = Vec::new();
    let mut checks: Vec<CheckResult> = Vec::new();

    // Build the full input prompt the model will see (matches backend format).
    let full_input = if case.system.is_empty() {
        format!("User: {}\n\nAssistant:", case.prompt)
    } else {
        format!("System: {}\n\nUser: {}\n\nAssistant:", case.system, case.prompt)
    };

    // Streaming trace: write header + input, then append each token as it's
    // generated. Also print tokens to stderr so the user sees live output.
    let on_token: Option<Box<dyn Fn(&str) + Send + Sync>> = match trace_path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let header = format!("--- {} ---\n{}", case.name, full_input);
            use std::io::Write;
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| f.write_all(header.as_bytes()));

            let dest = path.to_path_buf();
            Some(Box::new(move |word: &str| {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&dest)
                {
                    let _ = f.write_all(word.as_bytes());
                    let _ = f.flush();
                }
                let _ = std::io::stderr().write_all(word.as_bytes());
            }))
        }
        None => None,
    };

    let request = CompletionRequest {
        system: case.system.clone(),
        prompt: case.prompt.clone(),
        output_schema: None,
        grammar: case.grammar.clone(),
        temperature: case.temperature,
        max_tokens: case.max_tokens,
        estimated_prompt_tokens: 0,
        thinking: false,
        preserve_state: false,
        on_token,
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

            // Append oracle comparison to trace after output completes.
            // MATCH is a single checkmark line. MISMATCH shows both sides
            // side-by-side so divergence is immediately obvious.
            if let Some(trace_path) = trace_path {
                if let Some(ref oracle) = case.oracle {
                    use std::io::Write;
                    if output.contains(oracle) {
                        let note = format!("\n✓ oracle matches\n");
                        let _ = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(trace_path)
                            .and_then(|mut f| f.write_all(note.as_bytes()));
                    } else {
                        // Truncate long actual/oracle for display.
                        let trunc = |s: &str| -> String {
                            let s = s.trim();
                            if s.len() > 120 {
                                format!("{}…", &s[..120])
                            } else {
                                s.to_string()
                            }
                        };
                        let note = format!(
                            "\n✗ MISMATCH\n  actual: {}\n  oracle: {}\n",
                            trunc(&output),
                            trunc(oracle),
                        );
                        let _ = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(trace_path)
                            .and_then(|mut f| f.write_all(note.as_bytes()));
                    }
                }
            }

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
                input: full_input.clone(),
                output,
                latency_ms,
                token_usage: usage,
                tokens_per_sec,
                checks,
                errors,
                oracle: case.oracle.clone(),
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
                input: full_input.clone(),
                output: String::new(),
                latency_ms,
                token_usage: TokenUsage::default(),
                tokens_per_sec: 0.0,
                checks,
                errors,
                oracle: case.oracle.clone(),
            }
        }
    }
}

/// Run a suite of eval cases against a backend.
///
/// If `trace_path` is `Some`, each eval result streams tokens to that file
/// and to stderr in real time.
pub async fn run_suite<B: ModelBackend + Send + Sync>(
    suite_name: &str,
    backend: &B,
    cases: &[EvalCase],
    filter: Option<&str>,
    trace_path: Option<&std::path::Path>,
) -> EvalReport {
    let mut results = Vec::new();
    let start = Instant::now();

    for case in cases {
        if let Some(filter) = filter {
            if !case.name.contains(filter) && !case.description.contains(filter) && case.category.to_string() != filter {
                continue;
            }
        }
        let result = run_eval(backend, case, trace_path).await;
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

/// Write sidecar files alongside a trace: mismatches and oracle JSON.
pub fn write_sidecars(report: &EvalReport, trace_path: &std::path::Path) {
    // Derive sidecar paths from the trace path.
    // e.g. latest_trace.txt → latest.mismatches.txt, latest.oracle.json
    let parent = trace_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = trace_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("latest")
        .strip_suffix("_trace")
        .unwrap_or("latest");

    let mismatches_path = parent.join(format!("{stem}.mismatches.txt"));
    let oracle_path = parent.join(format!("{stem}.oracle.json"));

    // Mismatches file
    let mismatches: Vec<&EvalResult> = report
        .results
        .iter()
        .filter(|r| {
            if let Some(ref oracle) = r.oracle {
                !r.output.contains(oracle.as_str())
            } else {
                false
            }
        })
        .collect();

    let mut body = String::new();
    if mismatches.is_empty() {
        body.push_str("✓ no oracle mismatches\n");
    } else {
        for res in &mismatches {
            body.push_str(&format!("--- {} ---\n", res.name));
            body.push_str(&format!("  actual: {}\n", res.output.trim()));
            if let Some(ref oracle) = res.oracle {
                body.push_str(&format!("  oracle: {}\n", oracle.trim()));
            }
            body.push('\n');
        }
    }
    let _ = std::fs::create_dir_all(parent);
    let _ = std::fs::write(&mismatches_path, &body);
    println!("Mismatches:       {}", mismatches_path.display());

    // Oracle JSON map: { name: oracle, … }
    let mut oracle_map = serde_json::Map::new();
    for r in &report.results {
        if let Some(ref oracle) = r.oracle {
            oracle_map.insert(r.name.clone(), serde_json::Value::String(oracle.clone()));
        }
    }
    let oracle_json = serde_json::to_string_pretty(&oracle_map).unwrap_or_default();
    let _ = std::fs::write(&oracle_path, &oracle_json);
    println!("Oracles:          {}", oracle_path.display());
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
            temperature: 0.0,
            min_output_chars: 1,
            grammar: None,
            oracle: Some("Hello!".into()),
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
            temperature: 0.0,
            min_output_chars: 1,
            grammar: None,
            oracle: Some("42".into()),
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
            temperature: 0.0,
            min_output_chars: 20,
            grammar: None,
            oracle: Some("{\"colors\": [\"red\", \"green\", \"blue\"]}".into()),
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
            temperature: 0.0,
            min_output_chars: 30,
            grammar: None,
            oracle: Some("Step 1 complete\nStep 2 complete\nAll steps done".into()),
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
            temperature: 0.0,
            min_output_chars: 25,
            grammar: None,
            oracle: Some("The weather is sunny today.".into()),
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
            temperature: 0.0,
            min_output_chars: 50,
            grammar: None,
            // Free-form oracle: first sentence from a reference run.
            // The model may produce different wording each run at the GPU level,
            // so this is informational — pass/fail is from expected_hints/forbidden_strings.
            oracle: Some("A variable is a named storage location in a computer's memory that can hold a value. Variables are used to store and manipulate data in a program. They are typically declared with a name and a data type, and their values can be changed throughout the program's execution. Variables are essential for storing and manipulating data in a program, and they are used in a wide range of programming languages and applications.".into()),
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
            temperature: 0.0,
            min_output_chars: 40,
            grammar: None,
            oracle: Some("A robot named Pixel was programmed to paint, but it lacked the ability to express emotion. One day, it discovered a new color and began to paint with it, creating a masterpiece that touched the hearts of all who saw it.".into()),
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
            temperature: 0.0,
            min_output_chars: 20,
            grammar: None,
            oracle: Some("<think>Okay, the user wants me to list five different animals, each on a new line. Let me think about how to approach this.\nFirst, I need to make sure the animals are distinct and not similar. Maybe start with a common one like a dog, then move to a more unique one like a platypus. Next, a bird like a penguin, and then a mammal like a dolphin. Finally, a reptile like a snake. That should cover".into()),
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
            temperature: 0.0,
            min_output_chars: 20,
            grammar: None,
            oracle: Some("{\n  \"name\": \"John Doe\",\n  \"age\": 30,\n  \"city\": \"New York\"\n}".into()),
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
            temperature: 0.0,
            min_output_chars: 30,
            grammar: None,
            oracle: Some("1. A blanket or a picnic mat\n2. Food and drinks\n3. A cooler or a portable ice chest".into()),
            category: EvalCategory::Format,
        },

        // --- Newline handling --- //
        EvalCase {
            name: "newline_handling".into(),
            description:
                "Model outputs a three-line message when asked, each line separate.".into(),
            system: "You are a precise assistant. Always separate lines with line breaks.".into(),
            prompt: "Write exactly three lines:\nLine 1: Apples\nLine 2: Bananas\nLine 3: Cherries".into(),
            expected_hints: vec!["Apples".into(), "Bananas".into(), "Cherries".into()],
            forbidden_strings: vec![],
            max_tokens: 50,
            temperature: 0.0,
            min_output_chars: 20,
            grammar: None,
            oracle: Some("Apples\nBananas\nCherries".into()),
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
        temperature: 0.0,
        min_output_chars: 100,
        grammar: None,
        oracle: None,
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
        temperature: 0.0,
        min_output_chars: 20,
        grammar: None,
        oracle: None,
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
            temperature: 0.0,
            min_output_chars: 2,
            grammar: Some(yes_no.to_string()),
            oracle: None,
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
            temperature: 0.0,
            min_output_chars: 1,
            grammar: Some(digit.to_string()),
            oracle: None,
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
            temperature: 0.0,
            min_output_chars: 5,
            grammar: Some(paren_lit.to_string()),
            oracle: None,
            category: EvalCategory::Format,
        },
    ]
}

/// Grammar-constrained eval cases driven by JSON Schema.
///
/// Same shape as [`grammar_eval_cases`] but the grammar string is
/// *generated* at case-construction time from a JSON Schema, using
/// `rocore::jsonschema_to_gbnf::schema_to_gbnf`. This is the
/// end-to-end path the JSON-Schema -> GBNF converter exists to
/// enable: caller authors a JSON Schema, the suite compiles the
/// matching GBNF, schoolmarm accepts it on first parse.
///
/// Today the cases cover the same primitives the converter
/// supports (string/integer/number/boolean/null + enum); object
/// and array schemas would yield BadSchema and the build fails,
/// which is the right behavior.
#[cfg(feature = "grammar-rwkv")]
pub fn jsonschema_eval_cases() -> Vec<EvalCase> {
    use crate::jsonschema_to_gbnf;
    use serde_json::json;

    // 1) true/false boolean via Schema -> Boolean rule
    let bool_schema = json!({"type":"boolean"});
    let bool_grammar = jsonschema_to_gbnf::schema_to_gbnf("root", &bool_schema)
        .expect("boolean schema is supported");

    // 2) enum of three short layer labels for rwkv7
    let layer_schema = json!({"enum":["g1g","g1h","g1d"]});
    let layer_grammar = jsonschema_to_gbnf::schema_to_gbnf("root", &layer_schema)
        .expect("enum schema is supported");

    // 3) integer emit directly as Schema-driven grammar
    let int_schema = json!({"type":"integer"});
    let int_grammar = jsonschema_to_gbnf::schema_to_gbnf("root", &int_schema)
        .expect("integer schema is supported");

    vec![
        EvalCase {
            name: "json_schema_boolean".into(),
            description:
                "Boolean yes/no via JSON Schema ({\"type\":\"boolean\"}) -> GBNF at runtime.".into(),
            system: "".into(),
            prompt: "Answer with one word: true or false.".into(),
            expected_hints: vec![],
            forbidden_strings: vec![],
            max_tokens: 8,
            temperature: 0.0,
            min_output_chars: 4,
            grammar: Some(bool_grammar),
            oracle: None,
            category: EvalCategory::Format,
        },
        EvalCase {
            name: "json_schema_enum_layer".into(),
            description:
                "Enum of rwkv7 layer generations (g1g / g1h / g1d) via JSON Schema.".into(),
            system: "".into(),
            prompt: "Pick one of: g1g, g1h, g1d.".into(),
            expected_hints: vec![],
            forbidden_strings: vec![],
            max_tokens: 8,
            temperature: 0.0,
            min_output_chars: 3,
            grammar: Some(layer_grammar),
            oracle: None,
            category: EvalCategory::Format,
        },
        EvalCase {
            name: "json_schema_integer".into(),
            description: "Integer value via JSON Schema ({\"type\":\"integer\"}).".into(),
            system: "".into(),
            prompt: "Answer with one negative integer.".into(),
            expected_hints: vec![],
            forbidden_strings: vec![],
            max_tokens: 8,
            temperature: 0.0,
            min_output_chars: 1,
            grammar: Some(int_grammar),
            oracle: None,
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

/// Write a sidecar mismatches file — only evals whose output diverges from
/// the oracle.  Empty if none failed, so a `cat` shows nothing or a "no
/// mismatches" line.
pub fn write_mismatches(report: &EvalReport, path: impl AsRef<std::path::Path>) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    let mismatches: Vec<&EvalResult> = report
        .results
        .iter()
        .filter(|r| !r.passed || r.errors.iter().any(|e| e.contains("oracle")))
        .collect();

    let mut body = String::new();
    if mismatches.is_empty() {
        body.push_str("✓ no oracle mismatches\n");
    } else {
        for res in &mismatches {
            body.push_str(&format!("--- {} ---\n", res.name));
            body.push_str(&format!("Assistant: {}\n", res.output.trim()));
            body.push_str("✗ MISMATCH\n");
            // We no longer have the oracle text here directly — it lives on
            // EvalCase, not EvalResult.  We'll include the raw output instead
            // so the user can compare against the trace.
            body.push_str(&format!("  actual: {}\n", res.output.trim()));
            body.push('\n');
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)
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
                panic!("grammar in eval case ‘{}’ did not parse: {e:?}", case.name)
            });
        }
    }
}

