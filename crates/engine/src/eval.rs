//! Standalone model evaluation framework.
//!
//! Tests a [`ModelBackend`] directly on concrete capabilities: instruction
//! following, output coherence, repetition avoidance, throughput, etc.
//! Produces structured JSON reports.

use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::backend::ModelBackend;
use crate::types::{CompletionRequest, TokenUsage};

// ---------------------------------------------------------------------------
// Eval case definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    pub name: String,
    pub description: String,
    pub system: String,
    pub prompt: String,
    pub expected_hints: Vec<String>,
    pub forbidden_strings: Vec<String>,
    pub max_tokens: usize,
    pub temperature: f32,
    pub min_output_chars: usize,
    #[serde(default)]
    pub grammar: Option<String>,
    pub category: EvalCategory,
    #[serde(default)]
    pub oracle: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalCategory {
    Smoke,
    Instruction,
    Coherence,
    Repetition,
    Throughput,
    Format,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub name: String,
    pub description: String,
    pub category: EvalCategory,
    pub passed: bool,
    pub input: String,
    pub output: String,
    pub latency_ms: u64,
    pub token_usage: TokenUsage,
    pub tokens_per_sec: f64,
    pub checks: Vec<CheckResult>,
    pub errors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle: Option<String>,
}

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
        let pct = if self.total > 0 { (self.passed as f64 / self.total as f64) * 100.0 } else { 0.0 };
        format!(
            "Eval suite '{suite}': {passed}/{total} passed ({pct:.0}%) on backend '{backend}' in {ms}ms",
            suite = self.suite_name, passed = self.passed, total = self.total,
            pct = pct, backend = self.backend_name, ms = self.total_latency_ms,
        )
    }
}

// ---------------------------------------------------------------------------
// Running evals
// ---------------------------------------------------------------------------

pub async fn run_eval<B: ModelBackend + Send + Sync>(
    backend: &B,
    case: &EvalCase,
    trace_path: Option<&std::path::Path>,
) -> EvalResult {
    let mut errors: Vec<String> = Vec::new();
    let mut checks: Vec<CheckResult> = Vec::new();

    let full_input = if case.system.is_empty() {
        format!("User: {}\n\nAssistant:", case.prompt)
    } else {
        format!("System: {}\n\nUser: {}\n\nAssistant:", case.system, case.prompt)
    };

    let on_token: Option<Box<dyn Fn(&str) + Send + Sync>> = match trace_path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let header = format!("--- {} ---\n{}", case.name, full_input);
            use std::io::Write;
            let _ = std::fs::OpenOptions::new()
                .create(true).append(true).open(path)
                .and_then(|mut f| f.write_all(header.as_bytes()));

            let dest = path.to_path_buf();
            Some(Box::new(move |word: &str| {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true).append(true).open(&dest)
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
        session: None,
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

            if let Some(trace_path) = trace_path {
                if let Some(ref oracle) = case.oracle {
                    use std::io::Write;
                    let note = if output.contains(oracle) {
                        format!("\n✓ oracle matches\n")
                    } else {
                        let trunc = |s: &str| {
                            let s = s.trim();
                            if s.len() > 120 { format!("{}…", &s[..120]) } else { s.to_string() }
                        };
                        format!("\n✗ MISMATCH\n  actual: {}\n  oracle: {}\n", trunc(&output), trunc(oracle))
                    };
                    let _ = std::fs::OpenOptions::new()
                        .create(true).append(true).open(trace_path)
                        .and_then(|mut f| f.write_all(note.as_bytes()));
                }
            }

            // Checks
            let non_empty = !output.trim().is_empty();
            checks.push(CheckResult {
                name: "non_empty".into(), passed: non_empty,
                detail: if non_empty { format!("output length: {} chars", output.len()) } else { "output was empty".into() },
            });

            let min_len_ok = output.len() >= case.min_output_chars;
            checks.push(CheckResult {
                name: "min_output_length".into(), passed: min_len_ok,
                detail: if min_len_ok { format!("{} >= {} chars", output.len(), case.min_output_chars) } else { format!("{} < {} chars", output.len(), case.min_output_chars) },
            });

            for hint in &case.expected_hints {
                let found = output.to_lowercase().contains(&hint.to_lowercase());
                checks.push(CheckResult {
                    name: format!("hint: {hint}"), passed: found,
                    detail: if found { format!("found '{hint}' in output") } else { format!("expected '{hint}' not found in output") },
                });
            }

            for bad in &case.forbidden_strings {
                let found = output.contains(bad);
                checks.push(CheckResult {
                    name: format!("forbidden: {bad}"), passed: !found,
                    detail: if found { format!("found forbidden string '{bad}' in output") } else { format!("'{bad}' not found (good)") },
                });
            }

            let sentences: Vec<&str> = output.split(|c: char| c == '.' || c == '!' || c == '?')
                .map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            let repeats = if sentences.len() >= 4 {
                let unique: HashSet<&str> = sentences.iter().copied().collect();
                sentences.len() - unique.len()
            } else { 0 };
            checks.push(CheckResult {
                name: "repetition_check".into(), passed: repeats <= 1,
                detail: if repeats <= 1 { format!("{} repeated sentences out of {}", repeats, sentences.len()) } else { format!("{} repeated sentences out of {} — may be looping", repeats, sentences.len()) },
            });

            if usage.completion_tokens >= 10 {
                let tp_ok = if latency_ms == 0 { true } else { tokens_per_sec >= 1.0 };
                checks.push(CheckResult {
                    name: "throughput".into(), passed: tp_ok,
                    detail: format!("{:.1} tok/s ({} tokens in {}ms)", tokens_per_sec, usage.completion_tokens, latency_ms),
                });
            }

            let passed = checks.iter().all(|c| c.passed);
            info!(eval = case.name, passed, latency_ms, tokens = usage.completion_tokens, "eval result");

            EvalResult {
                name: case.name.clone(), description: case.description.clone(),
                category: case.category, passed, input: full_input,
                output, latency_ms, token_usage: usage, tokens_per_sec,
                checks, errors, oracle: case.oracle.clone(),
            }
        }
        Err(e) => {
            errors.push(format!("{e}"));
            info!(eval = case.name, error = %e, "eval failed with error");
            EvalResult {
                name: case.name.clone(), description: case.description.clone(),
                category: case.category, passed: false, input: full_input,
                output: String::new(), latency_ms, token_usage: TokenUsage::default(),
                tokens_per_sec: 0.0, checks, errors, oracle: case.oracle.clone(),
            }
        }
    }
}

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

    let mut cat_map: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for r in &results {
        let cat = r.category.to_string();
        let entry = cat_map.entry(cat).or_insert((0, 0));
        entry.0 += 1;
        if r.passed { entry.1 += 1; }
    }
    let category_breakdown: Vec<CategoryBreakdown> = cat_map
        .into_iter()
        .map(|(category, (total, passed))| CategoryBreakdown { category, total, passed })
        .collect();

    EvalReport {
        suite_name: suite_name.to_string(), backend_name: backend.name().to_string(),
        total, passed, failed, total_latency_ms, results, category_breakdown,
    }
}

pub fn write_sidecars(report: &EvalReport, trace_path: &std::path::Path) {
    let parent = trace_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = trace_path.file_stem()
        .and_then(|s| s.to_str()).unwrap_or("latest")
        .strip_suffix("_trace").unwrap_or("latest");

    let mismatches_path = parent.join(format!("{stem}.mismatches.txt"));
    let oracle_path = parent.join(format!("{stem}.oracle.json"));

    let mismatches: Vec<&EvalResult> = report.results.iter()
        .filter(|r| if let Some(ref oracle) = r.oracle { !r.output.contains(oracle.as_str()) } else { false })
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

    let mut oracle_map = serde_json::Map::new();
    for r in &report.results {
        if let Some(ref oracle) = r.oracle {
            oracle_map.insert(r.name.clone(), serde_json::Value::String(oracle.clone()));
        }
    }
    let _ = std::fs::write(&oracle_path, serde_json::to_string_pretty(&oracle_map).unwrap_or_default());
    println!("Oracles:          {}", oracle_path.display());
}

pub fn write_report(path: impl AsRef<std::path::Path>, report: &EvalReport) -> Result<(), std::io::Error> {
    let json = serde_json::to_string_pretty(report)?;
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)
}

pub fn write_mismatches(report: &EvalReport, path: impl AsRef<std::path::Path>) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    let mismatches: Vec<&EvalResult> = report.results.iter()
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
            body.push_str(&format!("  actual: {}\n", res.output.trim()));
            if let Some(ref oracle) = res.oracle {
                body.push_str(&format!("  oracle: {}\n", oracle.trim()));
            }
            body.push('\n');
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)
}

pub fn print_report(report: &EvalReport) {
    println!();
    println!("═══════════════════════════════════════════════════");
    println!("  Eval Report: {}", report.suite_name);
    println!("  Backend:      {}", report.backend_name);
    println!("  Results:      {}/{} passed ({:.0}%)",
        report.passed, report.total,
        if report.total > 0 { report.passed as f64 / report.total as f64 * 100.0 } else { 0.0 });
    println!("  Total time:   {}ms", report.total_latency_ms);
    println!("═══════════════════════════════════════════════════");
    if !report.category_breakdown.is_empty() {
        println!("\n  Category Breakdown:");
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
                if !check.passed { println!("         ↳ {}: {}", check.name, check.detail); }
            }
            for err in &result.errors { println!("         ↳ error: {}", err); }
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
#[path = "tests/eval_suite.rs"]
mod tests;
