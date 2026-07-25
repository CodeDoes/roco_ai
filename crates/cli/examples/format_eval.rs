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

    // A short, complete assistant response in the same envelope.
    let assistant = match spec {
        FormatSpec::Xml { think: true } => {
            "midBren wiped soot from his brow.end\n<write>The hammer fell silent. Beneath the anvil, a hum.</write>".to_string()
        }
        FormatSpec::Xml { think: false } => {
            "<write>The hammer fell silent. Beneath the anvil, a hum.</write>".to_string()
        }
        FormatSpec::Marker { think: true } => {
            "THINK:Set the mood.\nWRITE:The hammer fell silent. Beneath the anvil, a hum.".to_string()
        }
        FormatSpec::Marker { think: false } => {
            "WRITE:The hammer fell silent. Beneath the anvil, a hum.".to_string()
        }
        FormatSpec::Separator { think: true } => {
            "---THINK---\nMood.\n---WRITE---\nThe hammer fell silent. A hum beneath the anvil.\n---END---".to_string()
        }
        FormatSpec::Separator { think: false } => {
            "---CHAPTER---\nThe hammer fell silent. A hum beneath the anvil.\n---END---".to_string()
        }
        FormatSpec::Bracket => {
            "[THINK:Mood]\n[WRITE:The hammer fell silent. A hum beneath the anvil.]".to_string()
        }
        FormatSpec::Prose => {
            "The hammer fell silent. Beneath the anvil, a hum stirred — faint, rhythmic, like a heartbeat buried in stone.".to_string()
        }
    };
    (user, assistant)
}

/// Bake a session with two few-shot examples in `spec`'s envelope so the
/// model's recurrent state has seen the format's closing delimiters.
async fn bake_format(backend: &RemoteBackend, spec: &FormatSpec, session: &str) {
    let system = "You write vivid fiction prose. Follow the requested format exactly.";
    // Two diverse pairs so the state learns FORMAT, not content.
    let (u1, a1) = bake_pair(spec);
    let outline2 = "Ch1: First Contact — Dr. Varma detects a signal.";
    let wiki2 = "Varma: radio astronomer, eleven hours into her shift.";
    let task2 = "Write Chapter 1: First Contact. Varma sees the signal spike.";
    let u2 = spec.build_prompt(outline2, wiki2, task2);
    let a2 = match spec {
        FormatSpec::Xml { think: true } => "midVarma blinked.end\n<write>The readout spiked. Eleven hours of static, then this.</write>".to_string(),
        FormatSpec::Xml { think: false } => "<write>The readout spiked. Eleven hours of static, then this.</write>".to_string(),
        FormatSpec::Marker { think: true } => "THINK:Discovery.\nWRITE:The readout spiked. Eleven hours of static, then this.".to_string(),
        FormatSpec::Marker { think: false } => "WRITE:The readout spiked. Eleven hours of static, then this.".to_string(),
        FormatSpec::Separator { think: true } => "---THINK---\nDiscovery.\n---WRITE---\nThe readout spiked. Eleven hours of static, then this.\n---END---".to_string(),
        FormatSpec::Separator { think: false } => "---CHAPTER---\nThe readout spiked. Eleven hours of static, then this.\n---END---".to_string(),
        FormatSpec::Bracket => "[THINK:Discovery]\n[WRITE:The readout spiked. Eleven hours of static, then this.]".to_string(),
        FormatSpec::Prose => "The readout spiked — eleven hours of static, then this clean deliberate pulse.".to_string(),
    };

    let _ = backend.feed_eos(Some(session.to_string())).await;
    let _ = backend
        .bake_state(
            session,
            system,
            &[(u1.as_str(), a1.as_str()), (u2.as_str(), a2.as_str())],
        )
        .await;
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
        "light", "dark", "sound", "smell", "cold", "warm", "glow", "hum", "silent",
        "flicker", "shadow", "bright", "soft", "hard", "taste", "touch", "feel",
        "see", "hear", "watch", "listen", "hot", "chill", "warmth", "echo",
        "glimmer", "whisper", "dim", "faint", "pulse", "beat", "rhythm",
    ];
    let lower = s.to_lowercase();
    SENSORS.iter().filter(|w| lower.contains(*w)).count()
}

// ═════════════════════════════════════════════════════════════════════════
// Runner
// ═════════════════════════════════════════════════════════════════════════

#[derive(Debug)]
struct RunResult {
    spec: FormatSpec,
    baked: bool,
    trial: usize,
    prose_words: usize,
    sensory: usize,
    repetition: f64,
    latency_ms: u64,
    format_ok: bool,
    think_contam: bool,
    snippet: String,
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
        Ok(r) => r.text,
        Err(e) => {
            let err = format!("ERROR: {e}");
            return RunResult {
                spec: *spec,
                baked: false,
                trial,
                prose_words: 0,
                sensory: 0,
                repetition: 0.0,
                latency_ms,
                format_ok: false,
                think_contam: false,
                snippet: err.clone(),
                error: Some(err),
            };
        }
    };

    let prose = spec.extract(&raw);
    RunResult {
        spec: *spec,
        baked: false, // filled in by caller
        trial,
        prose_words: word_count(&prose),
        sensory: sensory_count(&prose),
        repetition: repetition_ratio(&prose),
        latency_ms,
        format_ok: format_followed(&raw, spec),
        think_contam: has_think_contamination(&raw, spec),
        snippet: prose.chars().take(100).collect(),
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
            let sid = format!("fmt-{}-baked-t{t}", spec.name());
            // Re-bake only on the first trial; reuse the baked state after.
            if t == 0 {
                let _ = backend.feed_eos(Some(bake_sid.clone())).await;
                bake_format(&backend, spec, &bake_sid).await;
            }
            eprint!("  baked  trial {t}...");
            let mut r = run_one(&backend, spec, &sid, t).await;
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
    let mut groups: HashMap<(&'static str, bool), Vec<&RunResult>> = HashMap::new();
    for r in &results {
        groups.entry((r.spec.name(), r.baked)).or_default().push(r);
    }

    // Stable row order: by FormatSpec::all() order, then fresh before baked.
    let mut rows: Vec<(&'static str, bool, Vec<&RunResult>)> = Vec::new();
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
            .find(|r| r.spec == *spec && !r.baked && r.trial == 0)
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
}
