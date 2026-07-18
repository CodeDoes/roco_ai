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

#[derive(Serialize, Deserialize)]
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
    /// Pre-built BNF mask (overrides `grammar` when present). The eval
    /// harness does not construct masks itself (that would pull grammar-engine
    /// types into a compilation unit shared with the inference backend,
    /// triggering `error[E0275]`); callers that want constrained decoding
    /// build the mask from `grammar` + the backend's vocab and set this field.
    #[serde(skip)]
    pub bnf_mask: Option<Box<dyn crate::BnfMask>>,
    #[serde(default)]
    pub prefill: Option<String>,
    /// Optional named recurrent-state session to resume from (state-tuning).
    #[serde(default)]
    pub session: Option<String>,
    /// Whether to persist the resulting state into the session after this call.
    #[serde(default)]
    pub preserve_state: bool,
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
    Fim,
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
            Self::Fim => "fim",
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
    #[serde(skip)]
    pub latency_ms: u64,
    #[serde(skip)]
    pub token_usage: TokenUsage,
    #[serde(skip)]
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
    #[serde(skip)]
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

/// Build a BNF mask from a GBNF grammar string, using the backend's
/// vocabulary bytes.
///
pub async fn run_eval<B: ModelBackend + Send + Sync>(
    backend: &B,
    case: EvalCase,
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

    // Use a pre-built BNF mask if the case supplies one. The eval harness
    // (e.g. the `eval_suite` example) builds masks from `grammar` + the
    // backend's vocabulary bytes and sets `bnf_mask` before running, since
    // `roco-engine` itself must not depend on the grammar-engine crate
    // (doing so triggers `error[E0275]` against the inference backend).
    let bnf_mask = case.bnf_mask;

    let request = CompletionRequest {
        system: case.system.clone(),
        prompt: case.prompt.clone(),
        grammar: case.grammar.clone(),
        bnf_mask,
        prefill: case.prefill.clone(),
        session: case.session.clone(),
        preserve_state: case.preserve_state,
        temperature: case.temperature,
        max_tokens: case.max_tokens,
        on_token,
        ..Default::default()
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
                            if s.chars().count() > 120 { format!("{}…", s.chars().take(120).collect::<String>()) } else { s.to_string() }
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
                    detail: if tp_ok { "ok" } else { "too slow" }.into(),
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
    cases: Vec<EvalCase>,
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

/// Named recurrent-state session the few-shot FIM examples are baked into.
pub const FIM_SESSION: &str = "roco_fim";

/// Few-shot examples demonstrating the BEFORE/AFTER/INSERT bridge task.
///
/// These are baked into the recurrent state once (state-tuning), not re-fed
/// as prompt tokens on every completion call. Each example is a (context,
/// answer) pair: the `context` is what the model sees as a *user* turn
/// (the BEFORE/AFTER/INSERT scaffold), and the `answer` is the *assistant*
/// turn it should learn to produce (the bridging prose).
///
/// The bake replays these as proper (user, assistant) turns — mirroring
/// `bake_into_session`, the validated state-tune pattern. Feeding the
/// answer through `prompt` (not `prefill`) makes the baked state learn the
/// assistant role from the User:/Assistant: framing. The bridge cases then
/// resume from this state and, given a fresh BEFORE/AFTER/INSERT scaffold,
/// emit only the bridging prose.
///
/// (Zed's Zeta-2 FIM uses literal `<[fim-*]>` sentinels; RWKV-g1h
/// has no such tokens in its vocab, so we use a natural BEFORE/AFTER/INSERT
/// bridge instead.)
pub const FIM_FEW_SHOT: &[(&str, &str)] = &[
    (
        "BEFORE: The knight drew his sword and stepped forward.\nAFTER: the dragon took to the air, wings blotting out the sun.\nINSERT:",
        "He raised the blade, bracing for the clash.",
    ),
    (
        "BEFORE: She whispered a spell under her breath.\nAFTER: the ward flared to life around them.\nINSERT:",
        "Light gathered at her fingertips.",
    ),
    (
        "BEFORE: A lone cultivator climbed the mist-shrouded peak.\nAFTER: and the sect elders bowed in recognition.\nINSERT:",
        "At the summit he found the lost scripture waiting.",
    ),
];

/// Bake the few-shot FIM examples into a named session on the backend.
///
/// RWKV is not FIM-sentinel trained and re-feeding few-shot as prompt tokens
/// makes the base model continue the examples instead of answering. The
/// RWKV-correct technique is to bake the examples into the recurrent state
/// once (proper (user, assistant) turn replay) and resume from it. Returns
/// an error if the backend cannot bake.
///
/// The system instruction is included only on the first user turn (the actor
/// drops the `system` field for session/preserve_state calls), so the task
/// persona and the few-shot both live in the recurrent state.
pub async fn bake_fim_session<B: ModelBackend + Send + Sync>(
    backend: &B,
) -> Result<(), String> {
    let instruction = "You are RoCo, a collaborative story-writing assistant. \
        Given the text BEFORE the cursor and the text AFTER the cursor, write \
        ONLY the short passage that connects them (the INSERT field). Never \
        repeat the BEFORE or AFTER text, never use <fim> tags, never add \
        commentary.";
    for (i, (context, answer)) in FIM_FEW_SHOT.iter().enumerate() {
        let user_req = CompletionRequest {
            system: if i == 0 { instruction.to_string() } else { String::new() },
            prompt: context.to_string(),
            prefill: Some("<think></think>".to_string()),
            temperature: 0.0,
            max_tokens: 1,
            session: Some(FIM_SESSION.to_string()),
            preserve_state: true,
            ..Default::default()
        };
        if let Err(e) = backend.complete(user_req).await {
            return Err(format!("FIM bake (user turn {i}) failed: {e}"));
        }
        let asst_req = CompletionRequest {
            system: String::new(),
            prompt: answer.to_string(),
            prefill: Some("<think></think>".to_string()),
            temperature: 0.0,
            max_tokens: 1,
            session: Some(FIM_SESSION.to_string()),
            preserve_state: true,
            ..Default::default()
        };
        if let Err(e) = backend.complete(asst_req).await {
            return Err(format!("FIM bake (assistant turn {i}) failed: {e}"));
        }
    }
    Ok(())
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
