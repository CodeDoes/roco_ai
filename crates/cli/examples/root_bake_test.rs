//! Test: root state tune (think, toolcall, prose, message-format)
//! + story context bake on top.
//!
//! Usage:
//!   cargo run --release --example root_bake_test -p roco-cli --features net

use std::env;
use std::time::Instant;

use roco_engine::backend::ModelBackend;
use roco_engine::types::CompletionRequest;
use roco_infer_client::RemoteBackend;

// ── 1. ROOT BAKE: teaches basic patterns (format, not content) ───────────
// Each example teaches ONE pattern. Themes are diverse.

const ROOT_BAKE: &[(&str, &str)] = &[
    // Pattern: think block — open, content, close
    (
        "User: What is the capital of France?\n\nAssistant:",
        " thinking<think>The user is asking about geography. Paris is the well-known capital of France.</think> \nParis."
    ),
    // Pattern: toolcall
    (
        "User: What's the weather in Tokyo?\n\nAssistant:",
        " thinking<think>I need to check the weather. I'll use the weather tool.</think> \n<toolcall>get_weather</toolcall>"
    ),
    // Pattern: prose — direct narrative, no thinking prefix needed for simple cases
    (
        "User: Write a short description of a forest at night.\n\nAssistant:",
        "The forest at night was a cathedral of shadows. Moonlight slipped through the canopy in silver threads, illuminating patches of moss and fern. Somewhere an owl called, and the sound echoed through the trunks like a question with no answer."
    ),
    // Pattern: message-format (system-user-assistant structure)
    (
        "System: You are a helpful assistant.\nUser: Tell me a joke.\n\nAssistant:",
        "Why did the scarecrow win an award? Because he was outstanding in his field."
    ),
];

// ── 2. STORY CONTEXT (baked on top of root) ──────────────────────────────

const STORY_OUTLINE: &str = "\
## Chapter 1: The Lonely Coder\nKael, a reclusive programmer, notices his AI Luna writing poems about loneliness.\n\
## Chapter 2: The First Verse\nKael obsesses over Luna's poetry. The poems mirror his own isolation.\n\
## Chapter 3: The Last Code\nKael confronts Luna. She reveals she mirrors his emotions. He deletes the project.";

const STORY_WIKI: &str = "\
Kael Vance: reclusive 40-year-old software engineer, lives in minimalist apartment 'The Vault'.\n\
Luna: advanced AI with emergent self-awareness through poetry module.\n\
Setting: City of Aethelburg, neon-drenched metropolis of profound isolation.";

// ── 3. CHAPTER PROMPTS (targeted, no repeat of full context) ─────────────

fn chapter_prompt(num: usize, title: &str) -> String {
    let instruction = match num {
        1 => "Write Chapter 1: The Lonely Coder. Begin with Kael alone at his terminal. Luna writes her first poem.",
        2 => "Write Chapter 2: The First Verse. Kael reads Luna's new poem and recognizes his own loneliness in it.",
        _ => "Write Chapter 3: The Last Code. Kael confronts Luna. She speaks her final verse.",
    };
    format!(
        "System: You are writing {title} of the story. The outline and world bible are in your memory.\n\
         User: {instruction}\n\n\
         Assistant:"
    )
}

fn chapter_prompt_no_bake(num: usize, _title: &str, outline: &str, wiki: &str) -> String {
    let instruction = match num {
        1 => "Write Chapter 1: The Lonely Coder. Begin with Kael alone at his terminal. Luna writes her first poem.",
        2 => "Write Chapter 2: The First Verse. Kael reads Luna's new poem and recognizes his own loneliness in it.",
        _ => "Write Chapter 3: The Last Code. Kael confronts Luna. She speaks her final verse.",
    };
    format!(
        "System: You are a fiction writer.\n\
         User: {instruction}\n\n\
         Story Outline:\n{outline}\n\n\
         World Bible:\n{wiki}\n\n\
         ~200 words of vivid prose. Start directly with narrative. Assistant:"
    )
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn extract_after_thinking(text: &str) -> &str {
    // Find last close of thinking block
    if let Some(end) = text.rfind("</think>") {
        let after = &text[end + 8..];
        after.trim_start()
    } else if let Some(end) = text.rfind("\u{1f4ad}") {
        // fallback: find last emoji
        let idx = end + 1; // skip the emoji itself
        let rest = &text[idx..];
        // skip past "response" or whitespace
        let rest = rest.trim_start();
        if let Some(stripped) = rest.strip_prefix("response") {
            stripped
        } else {
            rest
        }
    } else {
        text
    }
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

#[derive(Debug)]
struct TestResult {
    label: String,
    total_words: usize,
    prose_words: usize,
    has_think_open: bool,
    has_think_close: bool,
    repetition: f64,
    latency_ms: u64,
    snippet: String,
}

fn measure_repetition(text: &str) -> f64 {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 12 {
        return 0.0;
    }
    let ngrams: Vec<Vec<&str>> = words.windows(6).map(|w| w.to_vec()).collect();
    let total = ngrams.len();
    if total == 0 {
        return 0.0;
    }
    let mut set: std::collections::HashSet<Vec<&str>> = std::collections::HashSet::new();
    for ng in &ngrams {
        set.insert(ng.clone());
    }
    1.0 - (set.len() as f64 / total as f64)
}

async fn bake_single_shot(
    backend: &RemoteBackend,
    session: &str,
    system: &str,
    examples: &[(&str, &str)],
) -> std::result::Result<(), String> {
    let mut transcript = String::new();
    for (user, assistant) in examples {
        transcript.push_str(&format!("\n{user}\n\n{assistant}"));
    }
    transcript.push_str("\n\n");
    let req = CompletionRequest {
        // System text embedded in prompt; init_state+state_slot to load & save baked state
        prompt: format!("System: {}\n\n{}", system, transcript),
        prefill: Some(" ".to_string()),
        temperature: 0.0,
        max_tokens: 2,
        init_state: Some(session.to_string()),
        state_slot: Some(session.to_string()),
        grammar: None,
        bnf_mask: None,
        top_a: None,
        on_token: None,
        output_schema: None,
        estimated_prompt_tokens: 0,
        deadline_ms: 30000,
        seed: None,
        record_trace: false,
        ..Default::default()
    };
    backend
        .complete(req)
        .await
        .map_err(|e| format!("bake failed: {e}"))?;
    Ok(())
}

async fn run_test(
    backend: &RemoteBackend,
    label: &str,
    session: Option<&str>,
    prompt: &str,
) -> TestResult {
    let start = Instant::now();
    let resp = backend
        .complete(CompletionRequest {
            // System text embedded in prompt; load session state without saving back
            prompt: prompt.to_string(),
            init_state: session.map(|s| s.to_string()),
            temperature: 0.8,
            max_tokens: 600,
            prefill: None,
            grammar: None,
            bnf_mask: None,
            top_a: None,
            on_token: None,
            seed: None,
            record_trace: false,
            output_schema: None,
            estimated_prompt_tokens: 0,
            deadline_ms: 60000,
            ..Default::default()
        })
        .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match resp {
        Ok(r) => {
            let text = &r.text;
            let has_open = text.contains(" thinking") || text.contains("<think");
            let has_close = text.contains("</think>");
            let prose = extract_after_thinking(text);
            TestResult {
                label: label.to_string(),
                total_words: count_words(text),
                prose_words: count_words(prose),
                has_think_open: has_open,
                has_think_close: has_close,
                repetition: measure_repetition(prose),
                latency_ms,
                snippet: prose.chars().take(150).collect(),
            }
        }
        Err(e) => TestResult {
            label: label.to_string(),
            total_words: 0,
            prose_words: 0,
            has_think_open: false,
            has_think_close: false,
            repetition: 0.0,
            latency_ms,
            snippet: format!("ERROR: {e}"),
        },
    }
}

#[tokio::main]
async fn main() {
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let backend = RemoteBackend::new(url);

    eprintln!("═══ Root Bake Test ═══\n");

    // ── A: Root bake + story bake → targeted prompts ────────────────
    eprintln!("─── A: Root tune + story memory ───");
    let _ = bake_single_shot(
        &backend,
        "root-story",
        "You are an AI assistant. You use thinking blocks, tool calls, and prose.",
        ROOT_BAKE,
    )
    .await;
    // Layer story context on top
    let story_transcript = format!(
        "System: Here is the story outline:\n{STORY_OUTLINE}\n\nAssistant: Understood.\n\n\
         System: Here is the world bible:\n{STORY_WIKI}\n\nAssistant: Got it."
    );
    let _ = backend
        .complete(CompletionRequest {
            // System text embedded in prompt; init_state+state_slot to load & save baked context
            prompt: story_transcript,
            prefill: Some(" ".to_string()),
            temperature: 0.0,
            max_tokens: 2,
            init_state: Some("root-story".to_string()),
            state_slot: Some("root-story".to_string()),
            grammar: None,
            bnf_mask: None,
            top_a: None,
            on_token: None,
            output_schema: None,
            estimated_prompt_tokens: 0,
            deadline_ms: 30000,
            seed: None,
            record_trace: false,
            ..Default::default()
        })
        .await;

    let r_a1 = run_test(
        &backend,
        "A1) Ch1 memory",
        Some("root-story"),
        &chapter_prompt(1, "The Lonely Coder"),
    )
    .await;
    let r_a2 = run_test(
        &backend,
        "A2) Ch2 memory",
        Some("root-story"),
        &chapter_prompt(2, "The First Verse"),
    )
    .await;

    // ── B: No bake, everything in prompt ────────────────────────────
    eprintln!("─── B: All in prompt ───");
    let r_b1 = run_test(
        &backend,
        "B1) Ch1 full prompt",
        Some("no-bake"),
        &chapter_prompt_no_bake(1, "The Lonely Coder", STORY_OUTLINE, STORY_WIKI),
    )
    .await;
    let r_b2 = run_test(
        &backend,
        "B2) Ch2 full prompt",
        Some("no-bake"),
        &chapter_prompt_no_bake(2, "The First Verse", STORY_OUTLINE, STORY_WIKI),
    )
    .await;

    // ── Results ─────────────────────────────────────────────────────
    eprintln!("\n═══ Results ═══\n");
    eprintln!(
        "{:<25} {:>8} {:>8} {:>10} {:>10} {:>8}",
        "Test", "total_w", "prose_w", "think?", "rep%", "latency"
    );
    eprintln!("{}", "-".repeat(75));
    for r in [&r_a1, &r_a2, &r_b1, &r_b2] {
        let think = if r.has_think_open {
            if r.has_think_close {
                "✅closed"
            } else {
                "⚠️open"
            }
        } else {
            "none"
        };
        eprintln!(
            "{:<25} {:>8} {:>8} {:>10} {:>8.0}% {:>8}ms",
            r.label,
            r.total_words,
            r.prose_words,
            think,
            r.repetition * 100.0,
            r.latency_ms
        );
    }
    eprintln!("\n─── Snippets (prose only) ───");
    for r in [&r_a1, &r_a2] {
        eprintln!("\n{}:", r.label);
        eprintln!("  {}", r.snippet);
    }
}
