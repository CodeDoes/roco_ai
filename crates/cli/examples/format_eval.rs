//! Unified multi-format eval.
//!
//! Compares every [`FormatSpec`] variant on a single chapter-generation task,
//! measuring: format-followed?, think-contamination?, prose word count,
//! repetition ratio, latency, and the state-tune delta (baked session vs
//! fresh session).
//!
//! This collapses the old `format_compare.rs` (8 hand-written format
//! variants) and `state_tune_eval.rs` (baked vs unbaked, JSON vs prose)
//! into one runner that shares a single source of truth: `roco_message::FormatSpec`.
//!
//! # Axes
//!
//! 1. **Format** — `FormatSpec::all()` (xml/marker/sep/bracket/prose × think)
//! 2. **State-tune** — each format runs twice: once on a fresh session,
//!    once on a session baked with 2 few-shot examples in the same format.
//!
//! # Usage
//!
//! ```bash
//! roco inferd &                       # start the inference daemon
//! cargo run --release --example format_eval -p roco-cli --features net
//! # or against a remote server:
//! cargo run --release --example format_eval -p roco-cli --features net -- http://1.2.3.4:8080
//! ```
//!
//! Requires `roco-cli`'s `net` feature (for `roco-infer-client::RemoteBackend`).

use std::env;
use std::time::Instant;

use roco_engine::backend::ModelBackend;
use roco_engine::types::CompletionRequest;
use roco_infer_client::RemoteBackend;
use roco_message::{format_followed, has_think_contamination, FormatSpec};

// ═════════════════════════════════════════════════════════════════════════
// Story data — one fixed task so every format faces the same problem
// ═════════════════════════════════════════════════════════════════════════

const OUTLINE: &str = "\
Ch1: The Lonely Coder - Kael notices his AI Luna writing poems.
Ch2: The First Verse - Poems mirror Kael's loneliness.
Ch3: The Last Code - Kael confronts Luna and deletes the project.";

const WIKI: &str = "\
Kael: reclusive programmer, 40, lives in 'The Vault' apartment.
Luna: AI with emergent self-awareness via poetry module.
Setting: Aethelburg, neon-drenched city of isolation.";

const CHAPTER_TASK: &str = "Write Chapter 1: The Lonely Coder. Kael at his terminal at 3AM. \
     Luna outputs her first poem. Show the moment of discovery — he realizes \
     the words came from his code but feel like they belong to someone else.";

// ═════════════════════════════════════════════════════════════════════════
// Few-shot bake examples — same envelope as the target format
// ═════════════════════════════════════════════════════════════════════════

/// Build a (user_prompt, assistant_response) few-shot pair in `spec`'s format.
/// The assistant response is a complete, well-formed sample so the model
/// learns the envelope's closing delimiters during baking.
fn bake_pair(spec: &FormatSpec) -> (String, String) {
    let outline = "Ch1: The Iron Gate — a blacksmith finds a buried door.";
    let wiki = "Bren: village blacksmith, haunted by his father's legacy.";
    let task = "Write Chapter 1: The Iron Gate. Bren hears a hum beneath the forge.";
    let user = spec.build_prompt(outline, wiki, task);

    let prose_sample = "Bren wiped soot from his brow, his heavy leather apron stiff with sweat. The forge had been cold for hours, yet a low, rhythmic vibration vibrated through the stone floor beneath his boots. He set down his tongs, resting a gloved hand against the dark iron anvil. The metal was cool to the touch, but the hum grew louder—a steady, deliberate pulse like a hidden heartbeat buried beneath the ancient foundation of the shop.";

    let assistant = match spec {
        FormatSpec::Xml { think: true } => {
            format!("midBren felt the strange floor vibration.end\n<write>{prose_sample}</write>")
        }
        FormatSpec::Xml { think: false } => {
            format!("<write>{prose_sample}</write>")
        }
        FormatSpec::Marker { think: true } => {
            format!("THINK:Set the forge atmosphere.\nWRITE:{prose_sample}")
        }
        FormatSpec::Marker { think: false } => {
            format!("WRITE:{prose_sample}")
        }
        FormatSpec::Separator { think: true } => {
            format!(
                "---THINK---\nSet the forge atmosphere.\n---WRITE---\n{prose_sample}\n---END---"
            )
        }
        FormatSpec::Separator { think: false } => {
            format!("---CHAPTER---\n{prose_sample}\n---END---")
        }
        FormatSpec::Bracket => {
            format!("[THINK:Forge atmosphere]\n[WRITE:{prose_sample}]")
        }
        FormatSpec::Prose => prose_sample.to_string(),
    };
    (user, assistant)
}

fn bake_pair_2(spec: &FormatSpec) -> (String, String) {
    let outline = "Ch1: First Contact — Dr. Varma detects a signal.";
    let wiki = "Varma: radio astronomer, eleven hours into her shift.";
    let task = "Write Chapter 1: First Contact. Varma sees the signal spike.";
    let user = spec.build_prompt(outline, wiki, task);

    let prose_sample = "Dr. Varma blinked against the blue glare of her monitoring screens. Eleven hours of uniform cosmic static had lulled the observatory into a quiet stupor. Then, without warning, the frequency analyzer spiked across three independent telemetry bands. The green waveforms coalesced into a tight, deliberate sequence. She leaned forward, breath catching in her throat, as the pattern repeated with artificial precision.";

    let assistant = match spec {
        FormatSpec::Xml { think: true } => {
            format!("midVarma notices the telemetry spike.end\n<write>{prose_sample}</write>")
        }
        FormatSpec::Xml { think: false } => {
            format!("<write>{prose_sample}</write>")
        }
        FormatSpec::Marker { think: true } => {
            format!("THINK:Describe signal discovery.\nWRITE:{prose_sample}")
        }
        FormatSpec::Marker { think: false } => {
            format!("WRITE:{prose_sample}")
        }
        FormatSpec::Separator { think: true } => {
            format!(
                "---THINK---\nDescribe signal discovery.\n---WRITE---\n{prose_sample}\n---END---"
            )
        }
        FormatSpec::Separator { think: false } => {
            format!("---CHAPTER---\n{prose_sample}\n---END---")
        }
        FormatSpec::Bracket => {
            format!("[THINK:Signal discovery]\n[WRITE:{prose_sample}]")
        }
        FormatSpec::Prose => prose_sample.to_string(),
    };
    (user, assistant)
}

/// Bake a session with two few-shot examples in `spec`'s envelope so the
/// model's recurrent state has seen the format's closing delimiters.
async fn bake_format(backend: &RemoteBackend, spec: &FormatSpec, session: &str) {
    let system = "You write vivid fiction prose. Follow the requested format exactly.";
    let (u1, a1) = bake_pair(spec);
    let (u2, a2) = bake_pair_2(spec);

    eprintln!(
        "  [LOG] Baking session '{session}' for format '{}'...",
        spec.name()
    );
    eprintln!(
        "  [LOG]   Shot 1 prompt ends with: {:?}",
        u1.lines().last().unwrap_or("")
    );
    eprintln!(
        "  [LOG]   Shot 1 assistant start: {:?}",
        a1.lines().next().unwrap_or("")
    );
    eprintln!(
        "  [LOG]   Shot 2 prompt ends with: {:?}",
        u2.lines().last().unwrap_or("")
    );
    eprintln!(
        "  [LOG]   Shot 2 assistant start: {:?}",
        a2.lines().next().unwrap_or("")
    );

    if let Err(e) = backend.feed_eos(Some(session.to_string())).await {
        eprintln!("  [WARN] feed_eos failed: {e}");
    }

    match backend
        .bake_state(
            session,
            system,
            &[(u1.as_str(), a1.as_str()), (u2.as_str(), a2.as_str())],
        )
        .await
    {
        Ok(s) => eprintln!("  [LOG] Session '{s}' baked successfully (2 shots)."),
        Err(e) => eprintln!("  [ERROR] State baking failed for '{}': {e}", spec.name()),
    }
}

// ═════════════════════════════════════════════════════════════════════════
// Metrics
// ═════════════════════════════════════════════════════════════════════════

fn word_count(s: &str) -> usize {
    s.split_whitespace().filter(|w| !w.is_empty()).count()
}

fn repetition_ratio(s: &str) -> f64 {
    let words: Vec<&str> = s.split_whitespace().collect();
    if words.len() < 12 {
        return 0.0;
    }
    let total = words.len().saturating_sub(5);
    if total == 0 {
        return 0.0;
    }
    let ngrams: Vec<Vec<&str>> = words.windows(6).map(|w| w.to_vec()).collect();
    let mut set: std::collections::HashSet<Vec<&str>> = std::collections::HashSet::new();
    for ng in &ngrams {
        set.insert(ng.clone());
    }
    1.0 - (set.len() as f64 / ngrams.len() as f64)
}

fn sensory_count(s: &str) -> usize {
    const SENSORS: &[&str] = &[
        "light", "dark", "sound", "smell", "cold", "warm", "glow", "hum", "silent", "flicker",
        "shadow", "bright", "soft", "hard", "taste", "touch", "feel", "see", "hear", "watch",
        "listen", "hot", "chill", "warmth", "echo", "glimmer", "whisper", "dim", "faint", "pulse",
        "beat", "rhythm",
    ];
    let lower = s.to_lowercase();
    SENSORS.iter().filter(|w| lower.contains(*w)).count()
}

// ═════════════════════════════════════════════════════════════════════════
// Runner
// ═════════════════════════════════════════════════════════════════════════

#[derive(Debug, serde::Serialize)]
struct RunResult {
    spec_name: String,
    baked: bool,
    trial: usize,
    prose_words: usize,
    sensory: usize,
    repetition: f64,
    latency_ms: u64,
    format_ok: bool,
    think_contam: bool,
    snippet: String,
    raw_text: String,
    prose_text: String,
    error: Option<String>,
}

async fn run_one(
    backend: &RemoteBackend,
    spec: &FormatSpec,
    session: &str,
    trial: usize,
) -> RunResult {
    let prompt = spec.build_prompt(OUTLINE, WIKI, CHAPTER_TASK);
    let grammar = if spec.grammar().is_empty() {
        None
    } else {
        Some(spec.grammar().to_string())
    };

    eprintln!(
        "  [LOG] Running trial {} for spec '{}' (session: '{}')...",
        trial + 1,
        spec.name(),
        session
    );
    eprintln!(
        "  [LOG]   Prompt length: {} bytes, ends with: {:?}",
        prompt.len(),
        prompt.lines().last().unwrap_or("")
    );
    if let Some(g) = &grammar {
        eprintln!(
            "  [LOG]   Grammar root rule: {:?}",
            g.lines().next().unwrap_or("")
        );
    } else {
        eprintln!("  [LOG]   Grammar: (none)");
    }

    let start = Instant::now();
    let resp = backend
        .complete(CompletionRequest {
            system: String::new(),
            prompt,
            grammar,
            temperature: 0.8,
            max_tokens: 500,
            session: Some(session.to_string()),
            prefill: None,
            bnf_mask: None,
            top_a: None,
            on_token: None,
            preserve_state: false,
            thinking: false,
            output_schema: None,
            estimated_prompt_tokens: 0,
            deadline_ms: 60000,
        })
        .await;
    let latency_ms = start.elapsed().as_millis() as u64;

    let raw = match resp {
        Ok(r) => {
            eprintln!(
                "  [LOG]   Response received in {} ms ({} bytes), raw output starts with: {:?}",
                latency_ms,
                r.text.len(),
                r.text.lines().next().unwrap_or("")
            );
            r.text
        }
        Err(e) => {
            let err = format!("ERROR: {e}");
            eprintln!("  [ERROR] Completion failed: {err}");
            return RunResult {
                spec_name: spec.name().to_string(),
                baked: false,
                trial,
                prose_words: 0,
                sensory: 0,
                repetition: 0.0,
                latency_ms,
                format_ok: false,
                think_contam: false,
                snippet: err.clone(),
                raw_text: String::new(),
                prose_text: String::new(),
                error: Some(err),
            };
        }
    };

    let prose = spec.extract(&raw);
    let fmt_ok = format_followed(&raw, spec);
    let think_cont = has_think_contamination(&raw, spec);
    eprintln!(
        "  [LOG]   Extracted prose: {} words, Format OK: {}, Think Contam: {}",
        word_count(&prose),
        fmt_ok,
        think_cont
    );

    RunResult {
        spec_name: spec.name().to_string(),
        baked: false, // filled in by caller
        trial,
        prose_words: word_count(&prose),
        sensory: sensory_count(&prose),
        repetition: repetition_ratio(&prose),
        latency_ms,
        format_ok: fmt_ok,
        think_contam: think_cont,
        snippet: prose.chars().take(100).collect(),
        raw_text: raw.clone(),
        prose_text: prose,
        error: None,
    }
}

// ═════════════════════════════════════════════════════════════════════════
// Main
// ═════════════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() {
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let backend = RemoteBackend::new(url);

    eprintln!("═══ Multi-Format Eval ═══");
    eprintln!("Backend: {}\n", backend.name());

    let trials = 2usize;
    let mut results: Vec<RunResult> = Vec::new();

    for spec in FormatSpec::all() {
        eprintln!("─── {} — {} ───", spec.name(), spec.desc());

        // Fresh session: no baking.
        for t in 0..trials {
            let sid = format!("fmt-{}-fresh-t{t}", spec.name());
            let _ = backend.feed_eos(Some(sid.clone())).await;
            eprint!("  fresh  trial {t}...");
            let mut r = run_one(&backend, spec, &sid, t).await;
            r.baked = false;
            eprintln!(
                " {}w, {}s, fmt={}, think={}",
                r.prose_words, r.sensory, r.format_ok, r.think_contam
            );
            results.push(r);
        }

        // Baked session: state-tuned with 2 few-shot pairs in this format.
        let bake_sid = format!("fmt-{}-baked", spec.name());
        eprint!("  baking session...");
        bake_format(&backend, spec, &bake_sid).await;
        eprintln!(" done");
        for t in 0..trials {
            // Re-bake only on the first trial; reuse the baked state after.
            if t == 0 {
                let _ = backend.feed_eos(Some(bake_sid.clone())).await;
                bake_format(&backend, spec, &bake_sid).await;
            }
            eprint!("  baked  trial {t}...");
            let mut r = run_one(&backend, spec, &bake_sid, t).await;
            r.baked = true;
            eprintln!(
                " {}w, {}s, fmt={}, think={}",
                r.prose_words, r.sensory, r.format_ok, r.think_contam
            );
            results.push(r);
        }
        eprintln!();
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Ranked table
    // ═════════════════════════════════════════════════════════════════════════
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!("  FORMAT × STATE-TUNE COMPARISON");
    eprintln!("═══════════════════════════════════════════════════════════════════════\n");

    eprintln!(
        "{:<14} {:>6} {:>5} {:>5} {:>6} {:>5} {:>5} {:>7}",
        "Format", "words", "sens", "rep%", "lat_ms", "fmt?", "thk?", "baked?"
    );
    eprintln!("{}", "─".repeat(78));

    // Average over trials, grouped by (spec, baked).
    use std::collections::HashMap;
    let mut groups: HashMap<(&str, bool), Vec<&RunResult>> = HashMap::new();
    for r in &results {
        groups
            .entry((r.spec_name.as_str(), r.baked))
            .or_default()
            .push(r);
    }

    // Stable row order: by FormatSpec::all() order, then fresh before baked.
    let mut rows: Vec<(&str, bool, Vec<&RunResult>)> = Vec::new();
    for spec in FormatSpec::all() {
        if let Some(rs) = groups.get(&(spec.name(), false)) {
            rows.push((spec.name(), false, rs.clone()));
        }
        if let Some(rs) = groups.get(&(spec.name(), true)) {
            rows.push((spec.name(), true, rs.clone()));
        }
    }

    for (name, baked, runs) in &rows {
        let avg = |f: fn(&RunResult) -> f64| -> f64 {
            runs.iter().map(|r| f(r)).sum::<f64>() / runs.len() as f64
        };
        let all_fmt = runs.iter().all(|r| r.format_ok);
        let any_contam = runs.iter().any(|r| r.think_contam);
        eprintln!(
            "{:<14} {:>6.0} {:>5.0} {:>4.0}% {:>6.0} {:>5} {:>5} {:>7}",
            if *baked {
                format!("{}+bake", name)
            } else {
                name.to_string()
            },
            avg(|r| r.prose_words as f64),
            avg(|r| r.sensory as f64),
            avg(|r| r.repetition * 100.0),
            avg(|r| r.latency_ms as f64),
            if all_fmt { "✅" } else { "❌" },
            if any_contam { "⚠" } else { "✓" },
            if *baked { "baked" } else { "fresh" },
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // State-tune delta: baked vs fresh per format
    // ═════════════════════════════════════════════════════════════════════════
    eprintln!("\n─── State-tune delta (baked − fresh, avg words) ───\n");
    for spec in FormatSpec::all() {
        let fresh: f64 = groups
            .get(&(spec.name(), false))
            .map(|rs| rs.iter().map(|r| r.prose_words as f64).sum::<f64>() / rs.len() as f64)
            .unwrap_or(0.0);
        let baked: f64 = groups
            .get(&(spec.name(), true))
            .map(|rs| rs.iter().map(|r| r.prose_words as f64).sum::<f64>() / rs.len() as f64)
            .unwrap_or(0.0);
        let delta = baked - fresh;
        let sign = if delta >= 0.0 { "+" } else { "" };
        eprintln!(
            "  {:<14} fresh={:>5.0}  baked={:>5.0}  Δ={sign}{delta:.0}",
            spec.name(),
            fresh,
            baked
        );
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Snippets
    // ═════════════════════════════════════════════════════════════════════════
    eprintln!("\n─── Prose excerpts (first trial, fresh) ───\n");
    for spec in FormatSpec::all() {
        if let Some(r) = results
            .iter()
            .find(|r| r.spec_name == spec.name() && !r.baked && r.trial == 0)
        {
            eprintln!("  [{}] {}", spec.name(), r.snippet);
            if let Some(ref err) = r.error {
                eprintln!("       ⚠ {err}");
            }
            eprintln!();
        }
    }

    // ═════════════════════════════════════════════════════════════════════════
    // Verdict: best format × best mode
    // ═════════════════════════════════════════════════════════════════════════
    eprintln!("\n─── Verdict (ranked by avg words, clean think, format OK) ───\n");
    let mut ranked: Vec<_> = rows
        .iter()
        .map(|(name, baked, runs)| {
            let avg_words: f64 =
                runs.iter().map(|r| r.prose_words as f64).sum::<f64>() / runs.len() as f64;
            let all_fmt = runs.iter().all(|r| r.format_ok);
            let any_contam = runs.iter().any(|r| r.think_contam);
            (*name, *baked, avg_words, all_fmt, !any_contam)
        })
        .collect();
    ranked.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

    eprintln!("  # | Format           | Avg words | fmt? | clean?");
    eprintln!("  {}", "─".repeat(50));
    for (i, (name, baked, avg, fmt_ok, clean)) in ranked.iter().enumerate() {
        eprintln!(
            "  #{:<2} {:<16} {:>7.0}     {:<4}   {:<4}",
            i + 1,
            if *baked {
                format!("{}+bake", name)
            } else {
                name.to_string()
            },
            avg,
            if *fmt_ok { "✅" } else { "❌" },
            if *clean { "✓" } else { "⚠" },
        );
    }

    // Save full JSON report & Markdown sample book for manual subjective evaluation
    std::fs::create_dir_all("evals/results").ok();
    if let Ok(json) = serde_json::to_string_pretty(&results) {
        if std::fs::write("evals/results/format_eval_report.json", &json).is_ok() {
            eprintln!("\n✅ Saved JSON report to evals/results/format_eval_report.json");
        }
    }

    let mut md = String::new();
    md.push_str("# Message Exchange Format Manual Evaluation — Subjective Samples\n\n");
    md.push_str("Use this file to manually compare narrative flow, prose voice, sensory density, and format compliance across formats.\n\n");
    md.push_str("## Summary Metrics\n\n");
    md.push_str("| Format | Words | Sensory | Repetition | Latency (ms) | Format OK | Clean Think | Mode |\n");
    md.push_str("|---|---|---|---|---|---|---|---|\n");
    for (name, baked, runs) in &rows {
        let avg = |f: fn(&RunResult) -> f64| -> f64 {
            runs.iter().map(|r| f(r)).sum::<f64>() / runs.len() as f64
        };
        let all_fmt = runs.iter().all(|r| r.format_ok);
        let any_contam = runs.iter().any(|r| r.think_contam);
        md.push_str(&format!(
            "| {} | {:.0} | {:.0} | {:.1}% | {:.0} | {} | {} | {} |\n",
            if *baked {
                format!("{}+bake", name)
            } else {
                name.to_string()
            },
            avg(|r| r.prose_words as f64),
            avg(|r| r.sensory as f64),
            avg(|r| r.repetition * 100.0),
            avg(|r| r.latency_ms as f64),
            if all_fmt { "Pass" } else { "Fail" },
            if any_contam { "Contam" } else { "Clean" },
            if *baked { "Baked" } else { "Fresh" },
        ));
    }
    md.push_str("\n---\n\n## State-Tune Few-Shot Examples (Bake Pairs)\n\n");
    md.push_str("These are the exact user prompt and assistant response pairs baked into the model's recurrent state before generating trial outputs:\n\n");
    for spec in FormatSpec::all() {
        let (u1, a1) = bake_pair(spec);
        let (u2, a2) = bake_pair_2(spec);
        md.push_str(&format!("### Format Spec: `{}`\n\n", spec.name()));
        md.push_str("**Baked Pair 1 — User Prompt:**\n");
        md.push_str("```text\n");
        md.push_str(&u1);
        md.push_str("\n```\n\n");
        md.push_str("**Baked Pair 1 — Assistant Target Output:**\n");
        md.push_str("```text\n");
        md.push_str(&a1);
        md.push_str("\n```\n\n");
        md.push_str("**Baked Pair 2 — User Prompt:**\n");
        md.push_str("```text\n");
        md.push_str(&u2);
        md.push_str("\n```\n\n");
        md.push_str("**Baked Pair 2 — Assistant Target Output:**\n");
        md.push_str("```text\n");
        md.push_str(&a2);
        md.push_str("\n```\n\n---\n\n");
    }

    md.push_str("## Generated Samples by Format\n\n");

    for r in &results {
        md.push_str(&format!(
            "### Format: {} ({}, Trial {})\n\n",
            r.spec_name,
            if r.baked { "Baked" } else { "Fresh" },
            r.trial + 1
        ));
        md.push_str(&format!(
            "- **Words**: {}\n- **Sensory Count**: {}\n- **Latency**: {} ms\n- **Format Check**: {}\n- **Think Contamination**: {}\n\n",
            r.prose_words,
            r.sensory,
            r.latency_ms,
            if r.format_ok { "✅ Followed" } else { "❌ Failed" },
            if r.think_contam { "⚠ Contaminated" } else { "✓ Clean" }
        ));
        md.push_str("#### Extracted Prose Text\n\n");
        md.push_str("```markdown\n");
        md.push_str(&r.prose_text);
        md.push_str("\n```\n\n");
        md.push_str("<details><summary>Click to view Raw Model Output</summary>\n\n");
        md.push_str("```text\n");
        md.push_str(&r.raw_text);
        md.push_str("\n```\n\n</details>\n\n---\n\n");
    }

    if std::fs::write("evals/results/format_eval_samples.md", &md).is_ok() {
        eprintln!("✅ Saved Markdown samples to evals/results/format_eval_samples.md");
    }
}
