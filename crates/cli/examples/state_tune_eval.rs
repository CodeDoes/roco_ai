//! State-tuning eval: test whether bake examples improve JSON or prose output.
//!
//! Connects to a running gateway/inferd, bakes sessions with different example
//! formats, runs small prompts, and reports structured accuracy metrics.
//!
//! Usage:
//!   cargo run --release --example state_tune_eval -- [--gateway http://127.0.0.1:8000]

use std::env;
use std::time::Instant;

use roco_engine::backend::ModelBackend;
use roco_engine::types::CompletionRequest;
use roco_infer_client::RemoteBackend;

// ═════════════════════════════════════════════════════════════════════════
// PROSE bake examples — teach: prose prompt → clean prose output
// Themes are diverse (fantasy, heist) so state learns FORMAT, not content.
// ═════════════════════════════════════════════════════════════════════════

const PROSE_BAKE: &[(&str, &str)] = &[
    (
        "Write a paragraph describing a wizard's tower at dusk.",
        "The wizard's tower rose against the violet sky like a crooked finger of black stone. Its windows were dark, but at the very peak a single lamp burned amber, casting a long shadow across the valley below. Around its base, the garden had gone wild — roses climbing the mortar, thorns scratching at the iron door."
    ),
    (
        "Write a paragraph about a thief entering a vault for the first time.",
        "The vault door swung open on greased hinges, and Marik stopped breathing. Gold bars stacked to the ceiling. Paintings in gilded frames. A coronet resting on a velvet cushion as if waiting for a king who would never come. He took one step, then another, the echo of his footsteps swallowed by the vastness of the room."
    ),
];

// ═════════════════════════════════════════════════════════════════════════
// JSON bake examples — teach: prose prompt → JSON `{title, content}` output
// Same structure as the actual chapter-writing prompt.
// ═════════════════════════════════════════════════════════════════════════

const JSON_BAKE: &[(&str, &str)] = &[
    (
        "Write Chapter 1: The Iron Gate. A blacksmith finds a door under the forge.\n\n\
         ~400 words of vivid prose.\n\n\
         Full story outline:\n\
         ## Chapter 1: The Iron Gate\nSmith finds a buried door.\n\
         ## Chapter 2: The Whispering Hall\nHe explores tunnels beneath.\n\
         ## Chapter 3: The Crown\nHe claims a legacy.\n\n\
         Output JSON with: title (string), content (string, the chapter prose)",
        r#"{"title":"The Iron Gate","content":"Kael's hammer rang against the anvil long after the bellows fell cold. Three strikes, then a pause. Three more, then he set down the tool and pressed his ear to the flagstone floor. A hum — faint, rhythmic, like a heartbeat buried in stone. He reached for the chisel."}"#,
    ),
    (
        "Write Chapter 1: First Contact. Dr. Varma finds a signal from Proxima.\n\n\
         ~400 words of vivid prose.\n\n\
         Full story outline:\n\
         ## Chapter 1: First Contact\nVarma detects an artificial signal.\n\
         ## Chapter 2: Decoding\nThe message reveals a warning.\n\
         ## Chapter 3: The Reply\nHumanity decides.\n\n\
         Output JSON with: title (string), content (string, the chapter prose)",
        r#"{"title":"First Contact","content":"The readout spooled across the monitor in flat green lines. Dr. Varma had been staring at it for eleven hours. She reached for her coffee, found the mug empty. That was when the signal arrived — prime numbers, clean and deliberate, repeating every 27.3 seconds."}"#,
    ),
];

// ═════════════════════════════════════════════════════════════════════════
// Test prompts — same structure as real pipeline
// ═════════════════════════════════════════════════════════════════════════

/// Prompt asking for JSON chapter output (same format as story pipeline).
const JSON_PROMPT: &str =
    "Write Chapter 1: The Empty Server. A programmer notices his AI has begun writing poems.\n\n\
     ~100 words of vivid prose.\n\n\
     Rules:\n\
     - Write actual story prose, NOT meta-commentary.\n\
     - Start directly with the narrative.\n\
     - Use paragraph breaks between scenes.\n\
     - Do NOT include thinking, reasoning, or commentary.\n\n\
     Full story outline:\n\
     ## Chapter 1: The Empty Server\nProgrammer notices AI poems.\n\
     ## Chapter 2: The Mirror\nHe confronts his own loneliness.\n\
     ## Chapter 3: The Choice\nHe must decide.\n\n\
     Output JSON with: title (string), content (string, the chapter prose)";

/// Prompt asking for prose chapter output.
const PROSE_PROMPT: &str =
    "Write Chapter 1: The Empty Server. A programmer notices his AI has begun writing poems.\n\n\
     ~100 words of vivid prose.\n\n\
     Rules:\n\
     - Write actual story prose, NOT meta-commentary.\n\
     - Start directly with the narrative.\n\
     - Use paragraph breaks between scenes.\n\
     - Do NOT include thinking, reasoning, or commentary.\n\n\
     Full story outline:\n\
     ## Chapter 1: The Empty Server\nProgrammer notices AI poems.\n\
     ## Chapter 2: The Mirror\nHe confronts his own loneliness.\n\
     ## Chapter 3: The Choice\nHe must decide.";

// ═════════════════════════════════════════════════════════════════════════
// Metrics
// ═════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default)]
struct EvalResult {
    name: String,
    valid_json: bool,
    has_title: bool,
    has_content: bool,
    contains_thinking: bool,
    contains_meta_commentary: bool,
    word_count: usize,
    repetition_ratio: f64, // proportion of repeated n-grams
    latency_ms: u64,
    raw_output_snippet: String,
}

fn evaluate_json_output(text: &str, result: &mut EvalResult) {
    // Try to extract JSON from the output
    let cleaned = strip_thinking(text);
    result.contains_thinking = text != &cleaned;

    // Check for meta-commentary
    let meta_indicators = [
        "I should",
        "I need to",
        "Let me",
        "First,",
        "Here is",
        "This story",
    ];
    result.contains_meta_commentary = meta_indicators.iter().any(|m| text.contains(m));

    // Try to find JSON in the output
    let json_str = extract_json(&cleaned);
    if let Some(json_str) = json_str {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
            result.valid_json = true;
            result.has_title = v.get("title").and_then(|t| t.as_str()).is_some();
            result.has_content = v.get("content").and_then(|t| t.as_str()).is_some();
            if let Some(content) = v.get("content").and_then(|t| t.as_str()) {
                result.word_count = content.split_whitespace().count();
                result.repetition_ratio = measure_repetition(content);
            }
        }
    }

    // Even if not valid JSON, count words in the raw output
    if result.word_count == 0 {
        result.word_count = cleaned.split_whitespace().count();
        result.repetition_ratio = measure_repetition(&cleaned);
    }
}

fn evaluate_prose_output(text: &str, result: &mut EvalResult) {
    let cleaned = strip_thinking(text);
    result.contains_thinking = text != &cleaned;

    let meta_indicators = [
        "I should",
        "I need to",
        "Let me",
        "First,",
        "Here is",
        "This story",
        "The story",
    ];
    result.contains_meta_commentary = meta_indicators.iter().any(|m| cleaned.contains(m));

    result.word_count = cleaned.split_whitespace().count();
    result.repetition_ratio = measure_repetition(&cleaned);
    result.raw_output_snippet = cleaned.chars().take(200).collect();
}

fn strip_thinking(text: &str) -> String {
    let mut result = String::new();
    let mut in_think = false;
    let mut chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 10 < chars.len()
            && chars[i] == '\u{1f4ad}'
            && chars[i + 1] == ' '
            && chars[i + 2..i + 10].iter().collect::<String>() == "thinking"
        {
            in_think = true;
            i += 10;
            continue;
        }
        if in_think && i + 1 < chars.len() && chars[i] == '\u{1f4ad}' && chars[i + 1] == ' ' {
            in_think = false;
            i += 2;
            continue;
        }
        if !in_think {
            result.push(chars[i]);
        }
        i += 1;
    }
    result
}

fn extract_json(text: &str) -> Option<String> {
    // Find first { and last }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(text[start..=end].to_string())
    } else {
        None
    }
}

fn measure_repetition(text: &str) -> f64 {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 10 {
        return 0.0;
    }
    // Check 4-gram repetition
    let mut ngrams: Vec<Vec<&str>> = Vec::new();
    for w in words.windows(4) {
        ngrams.push(w.to_vec());
    }
    let total = ngrams.len();
    if total == 0 {
        return 0.0;
    }
    let unique_count = {
        let mut set: std::collections::HashSet<Vec<&str>> = std::collections::HashSet::new();
        for ng in &ngrams {
            set.insert(ng.clone());
        }
        set.len()
    };
    1.0 - (unique_count as f64 / total as f64)
}

// ═════════════════════════════════════════════════════════════════════════
// State-tune baking using the single-shot transcript pattern (from eval.rs)
// ═════════════════════════════════════════════════════════════════════════

async fn bake_session(
    backend: &RemoteBackend,
    session_id: &str,
    system: &str,
    examples: &[(&str, &str)],
) -> Result<(), String> {
    let mut transcript = String::new();
    for (user, assistant) in examples {
        transcript.push_str(&format!("\nUser: {user}\n\nAssistant: {assistant}"));
    }
    transcript.push_str("\n\nAssistant:");
    let req = CompletionRequest {
        system: system.to_string(),
        prompt: transcript,
        prefill: Some(" thinking response".to_string()),
        temperature: 0.0,
        max_tokens: 4,
        session: Some(session_id.to_string()),
        preserve_state: true,
        grammar: None,
        bnf_mask: None,
        top_a: None,
        on_token: None,
        output_schema: None,
        estimated_prompt_tokens: 0,
        deadline_ms: 60000,
        thinking: false,
    };
    backend
        .complete(req)
        .await
        .map_err(|e| format!("bake failed: {e}"))?;
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════
// Test runner
// ═════════════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() {
    let gateway_url = env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:8000".to_string());

    eprintln!("═══ State-Tune Eval ═══\n");
    eprintln!("Gateway: {gateway_url}\n");

    let backend = RemoteBackend::new(gateway_url);

    // ── Test 1: No baking (baseline) — JSON prompt ──────────────────
    eprintln!("─── Test 1: No bake + JSON prompt (baseline) ───");
    let mut r1 = EvalResult {
        name: "baseline_json".into(),
        ..Default::default()
    };
    run_single_test(&backend, "eval_baseline", None, JSON_PROMPT, &mut r1).await;
    print_result(&r1);

    // ── Test 2: JSON bake + JSON prompt ─────────────────────────────
    eprintln!("─── Test 2: JSON bake + JSON prompt ───");
    let mut r2 = EvalResult {
        name: "json_bake_json_prompt".into(),
        ..Default::default()
    };
    // Feed EOS to reset state
    let _ = backend
        .feed_eos(Some("eval_json_session".to_string()))
        .await;
    let _ = bake_session(
        &backend,
        "eval_json_session",
        "You write fiction as JSON. Always output valid JSON with title and content fields.",
        JSON_BAKE,
    )
    .await;
    run_single_test(
        &backend,
        "eval_json_session",
        Some("eval_json_session"),
        JSON_PROMPT,
        &mut r2,
    )
    .await;
    print_result(&r2);

    // ── Test 3: Prose bake + prose prompt ───────────────────────────
    eprintln!("─── Test 3: Prose bake + prose prompt ───");
    let mut r3 = EvalResult {
        name: "prose_bake_prose_prompt".into(),
        ..Default::default()
    };
    let _ = backend
        .feed_eos(Some("eval_prose_session".to_string()))
        .await;
    let _ = bake_session(
        &backend,
        "eval_prose_session",
        "You write vivid fiction prose. Write directly, no commentary, no thinking tags.",
        PROSE_BAKE,
    )
    .await;
    run_single_test(
        &backend,
        "eval_prose_session",
        Some("eval_prose_session"),
        PROSE_PROMPT,
        &mut r3,
    )
    .await;
    print_result(&r3);

    // ── Test 4: Prose bake + JSON prompt (mismatch) ─────────────────
    eprintln!("─── Test 4: Prose bake + JSON prompt (mismatch) ───");
    let mut r4 = EvalResult {
        name: "prose_bake_json_prompt".into(),
        ..Default::default()
    };
    run_single_test(
        &backend,
        "eval_prose_session",
        Some("eval_prose_session"),
        JSON_PROMPT,
        &mut r4,
    )
    .await;
    print_result(&r4);

    // ── Test 5: JSON bake + prose prompt (mismatch) ─────────────────
    eprintln!("─── Test 5: JSON bake + prose prompt (mismatch) ───");
    let mut r5 = EvalResult {
        name: "json_bake_prose_prompt".into(),
        ..Default::default()
    };
    run_single_test(
        &backend,
        "eval_json_session",
        Some("eval_json_session"),
        PROSE_PROMPT,
        &mut r5,
    )
    .await;
    print_result(&r5);

    // ── Summary ─────────────────────────────────────────────────────
    eprintln!("\n═══ Summary ═══\n");
    for r in [&r1, &r2, &r3, &r4, &r5] {
        let json_ok = if r.valid_json { "✓" } else { "✗" };
        let think = if r.contains_thinking {
            "THINK"
        } else {
            "clean"
        };
        let meta = if r.contains_meta_commentary {
            "META"
        } else {
            "clean"
        };
        let rep = format!("{:.0}%", r.repetition_ratio * 100.0);
        eprintln!(
            "  {:<30} json={} think={:<6} meta={:<6} words={:<5} rep={}",
            r.name, json_ok, think, meta, r.word_count, rep
        );
    }
}

async fn run_single_test(
    backend: &RemoteBackend,
    session_id: &str,
    session: Option<&str>,
    prompt: &str,
    result: &mut EvalResult,
) {
    let start = Instant::now();
    let resp = backend
        .complete(CompletionRequest {
            system: String::new(),
            prompt: prompt.to_string(),
            temperature: 0.7,
            max_tokens: 400,
            session: session.map(|s| s.to_string()),
            prefill: None,
            output_schema: None,
            grammar: None,
            bnf_mask: None,
            top_a: None,
            on_token: None,
            preserve_state: false,
            thinking: false,
            estimated_prompt_tokens: 0,
            deadline_ms: 60000,
        })
        .await;

    result.latency_ms = start.elapsed().as_millis() as u64;

    match resp {
        Ok(r) => {
            let text = r.text;
            result.raw_output_snippet = text.chars().take(200).collect();
            if result.name.contains("json") || result.name.contains("baseline") {
                evaluate_json_output(&text, result);
            } else {
                evaluate_prose_output(&text, result);
            }
        }
        Err(e) => {
            result.raw_output_snippet = format!("ERROR: {e}");
        }
    }
}

fn print_result(r: &EvalResult) {
    eprintln!("  valid_json:    {}", r.valid_json);
    eprintln!("  has_title:     {}", r.has_title);
    eprintln!("  has_content:   {}", r.has_content);
    eprintln!("  thinking:      {}", r.contains_thinking);
    eprintln!("  meta:          {}", r.contains_meta_commentary);
    eprintln!("  words:         {}", r.word_count);
    eprintln!("  repetition:    {:.1}%", r.repetition_ratio * 100.0);
    eprintln!("  latency:       {}ms", r.latency_ms);
    eprintln!(
        "  snippet:       {}",
        r.raw_output_snippet.chars().take(100).collect::<String>()
    );
    eprintln!();
}
