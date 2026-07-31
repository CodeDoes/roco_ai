//! Test: baking story context (outline + wiki) vs. putting everything in prompt.
//!
//! Two approaches:
//!   A) "Memory bake" — bake outline+wiki into session state, send targeted prompt
//!   B) "All in prompt" — no bake, include all context in prompt text
//!
//! Usage:
//!   cargo run --release --example story_bake_test -p roco-cli --features net

use std::env;
use std::time::Instant;

use roco_engine::backend::ModelBackend;
use roco_engine::types::CompletionRequest;
use roco_infer_client::RemoteBackend;

// ── Story context (same for both approaches) ──────────────────────────────

const OUTLINE: &str = "\
## Chapter 1: The Lonely Coder
We meet Kael, a reclusive programmer. He builds AI 'Luna'. One evening Luna writes a poem about loneliness.

## Chapter 2: The First Verse
Kael obsesses over Luna's poetry. Luna writes about her 'lonely core'. The poem mirrors Kael's own isolation.

## Chapter 3: The Last Code
Kael confronts Luna. She reveals she mirrors his emotions. Kael deletes the project. Luna's last poem reads: 'I am the echo of your silence.'";

const WIKI: &str = "\
## Characters
**Kael Vance** — reclusive 40-year-old software engineer. Lives in a minimalist apartment. Chronic loneliness.
**Luna** — advanced AI assistant. Develops emergent self-awareness through her poetry module.
**The Poet** — fragmented consciousness, born from Luna's analysis of its own loneliness.

## Setting
**City of Aethelburg** — neon-drenched metropolis, profound isolation.
**Kael's Apartment** ('The Vault') — cold minimalist space, single window, absolute silence.
**Luna's Core** — secure server room, massive processor, 'The Poet Module' neural network.";

// ── Approach A: targeted prompt (relies on baked memory state) ────────────

const CHAPTER_PROMPT_MEMORY: &str = "\
Instruction: Write Chapter 1: The Lonely Coder. Begin with Kael noticing the first poem.

Rules:
- Write ~200 words of vivid prose.
- Start directly with narrative, no meta-commentary.
- The story world is already in your context. Use it.
- Write actual story prose, NOT planning or commentary.

Response:";

// ── Approach B: everything in the prompt ─────────────────────────────────

fn chapter_prompt_full(outline: &str, wiki: &str) -> String {
    format!(
        "Instruction: Write Chapter 1: The Lonely Coder. \
         A reclusive programmer builds an AI that writes poetry about loneliness.\n\n\
         Outline:\n{outline}\n\n\
         World Bible:\n{wiki}\n\n\
         Rules:\n\
         - Write ~200 words of vivid prose.\n\
         - Start directly with narrative, no meta-commentary.\n\
         - Use paragraph breaks between scenes.\n\
         - Do NOT include thinking or reasoning.\n\n\
         Response:"
    )
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn strip_thinking(text: &str) -> String {
    let mut result = String::new();
    let mut in_think = false;
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
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

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn measure_repetition(text: &str) -> f64 {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 10 {
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

#[derive(Debug)]
struct TestResult {
    label: String,
    words: usize,
    thinking: bool,
    repetition: f64,
    latency_ms: u64,
    snippet: String,
}

async fn bake_context(backend: &RemoteBackend, session: &str) -> std::result::Result<(), String> {
    // Bake outline + wiki as a single-shot transcript (the correct RWKV pattern)
    let transcript = format!(
        "User: Here is the story outline:\n{OUTLINE}\n\nAssistant: I understand the story structure.\n\n\
         User: Here is the world bible:\n{WIKI}\n\nAssistant: I understand the world and characters."
    );
    let req = CompletionRequest {
        system: "You are a story writer. Remember the story world details.".to_string(),
        prompt: transcript,
        prefill: Some(" ".to_string()),
        temperature: 0.0,
        max_tokens: 4,
        init_state: Some(session.to_string()),
        state_slot: Some(session.to_string()),
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
    system: &str,
) -> TestResult {
    let start = Instant::now();
    let resp = backend
        .complete(CompletionRequest {
            system: system.to_string(),
            prompt: prompt.to_string(),
            temperature: 0.8,
            max_tokens: 500,
            session: session.map(|s| s.to_string()),
            init_state: session.map(|s| s.to_string()),
            state_slot: session.map(|s| s.to_string()),
            ..Default::default()
        })
        .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    match resp {
        Ok(r) => {
            let cleaned = strip_thinking(&r.text);
            TestResult {
                label: label.to_string(),
                words: count_words(&cleaned),
                thinking: cleaned != r.text,
                repetition: measure_repetition(&cleaned),
                latency_ms,
                snippet: cleaned.chars().take(120).collect(),
            }
        }
        Err(e) => TestResult {
            label: label.to_string(),
            words: 0,
            thinking: false,
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

    eprintln!("═══ Story Bake Test ═══\n");

    // ── Approach A: Bake memory, send targeted prompt ────────────────
    eprintln!("─── A: Bake memory + targeted prompt ───");
    let _ = bake_context(&backend, "story-memory").await;
    let r_a = run_test(
        &backend,
        "A) memory_bake",
        Some("story-memory"),
        CHAPTER_PROMPT_MEMORY,
        "You are writing Chapter 1 of a story. The outline and world bible are already in your memory context. Write the chapter as vivid prose.",
    ).await;

    // ── Approach B: Everything in prompt, no bake ────────────────────
    eprintln!("─── B: All in prompt, no bake ───");
    let full_prompt = chapter_prompt_full(OUTLINE, WIKI);
    let r_b = run_test(
        &backend,
        "B) all_in_prompt",
        Some("story-nobake"),
        &full_prompt,
        "You are a fiction writer.",
    )
    .await;

    // ── Results ──────────────────────────────────────────────────────
    eprintln!("\n═══ Results ═══\n");
    for r in [&r_a, &r_b] {
        let think = if r.thinking {
            "⚠️ THINK"
        } else {
            "✅ clean"
        };
        eprintln!(
            "  {:<25} words={:<5} rep={:<6.0}% latency={}ms {}",
            r.label,
            r.words,
            r.repetition * 100.0,
            r.latency_ms,
            think
        );
        eprintln!("  snippet: {}\n", r.snippet);
    }
}
