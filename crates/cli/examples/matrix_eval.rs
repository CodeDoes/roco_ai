//! Matrix evaluation example — tests 8-way combinations of:
//!   - BNF vs No-BNF
//!   - State (Baked) vs No-State (Fresh)
//!   - Think vs No-Think
//!
//! Run with:
//!   cargo run --release --example matrix_eval -p roco-cli --features net

use roco_engine::{CompletionRequest, ModelBackend};
use roco_infer_client::RemoteBackend;
use roco_protocol::FormatSpec;
use std::env;
use std::time::Instant;

const OUTLINE: &str = r#"
Chapter 1: The Lonely Coder
- Scene 1: Kael works late in his neon-lit apartment, writing code.
- Scene 2: He discovers an unexpected file named 'luna_journal.log'.
- Scene 3: The file contains poems written by the city AI, expressing loneliness.
"#;

const WIKI: &str = r#"
Setting: Aethelburg, neon-drenched city of isolation.
Protagonist: Kael, quiet programmer, age 28.
"#;

const TASK: &str =
    "Write Chapter 1 based on the outline and world wiki. Write vivid fiction prose.";

#[tokio::main]
async fn main() {
    let url = env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let backend = RemoteBackend::new(url);

    println!("═══ 8-Way Permutation Matrix Eval (Focus: XML Envelope) ═══");
    println!("Backend: {}\n", backend.name());

    let spec_think = FormatSpec::Xml { think: true };
    let spec_direct = FormatSpec::Xml { think: false };

    let session_3 = "matrix-session-3-nobnf-state-nothink";
    let session_4 = "matrix-session-4-nobnf-state-think";
    let session_7 = "matrix-session-7-bnf-state-nothink";
    let session_8 = "matrix-session-8-bnf-state-think";

    println!("─── Baking Sessions ───");
    bake(&backend, &spec_direct, session_3).await;
    bake(&backend, &spec_think, session_4).await;
    bake(&backend, &spec_direct, session_7).await;
    bake(&backend, &spec_think, session_8).await;
    println!("✓ Sessions baked.\n");

    let combinations = vec![
        (
            "1. nobnf + nostate + nothink",
            &spec_direct,
            "matrix-session-1",
            false,
            false,
            false,
        ),
        (
            "2. nobnf + nostate + think",
            &spec_think,
            "matrix-session-2",
            false,
            false,
            true,
        ),
        (
            "3. nobnf + state   + nothink",
            &spec_direct,
            session_3,
            false,
            true,
            false,
        ),
        (
            "4. nobnf + state   + think",
            &spec_think,
            session_4,
            false,
            true,
            true,
        ),
        (
            "5. bnf   + nostate + nothink",
            &spec_direct,
            "matrix-session-5",
            true,
            false,
            false,
        ),
        (
            "6. bnf   + nostate + think",
            &spec_think,
            "matrix-session-6",
            true,
            false,
            true,
        ),
        (
            "7. bnf   + state   + nothink",
            &spec_direct,
            session_7,
            true,
            true,
            false,
        ),
        (
            "8. bnf   + state   + think",
            &spec_think,
            session_8,
            true,
            true,
            true,
        ),
    ];

    println!(
        "{:<32} | {:<7} | {:<6} | {:<12} | {:<80}",
        "Combination", "Words", "Ms", "Raw Start", "Prose Snippet"
    );
    println!("{}", "─".repeat(145));

    for (name, spec, session, use_bnf, is_baked, is_think) in combinations {
        let prompt = spec.build_prompt(OUTLINE, WIKI, TASK);
        let grammar = if use_bnf {
            Some(spec.grammar().to_string())
        } else {
            None
        };

        if !is_baked {
            let _ = backend.feed_eos(Some(session.to_string())).await;
        }

        let start = Instant::now();
        let resp = backend
            .complete(CompletionRequest {
                system: String::new(),
                prompt,
                grammar,
                temperature: 0.8,
                max_tokens: 1000,
                session: Some(session.to_string()),
                prefill: None,
                bnf_mask: None,
                top_a: None,
                on_token: None,
                preserve_state: false,
                thinking: is_think,
                output_schema: None,
                estimated_prompt_tokens: 0,
                deadline_ms: 60000,
            })
            .await;
        let ms = start.elapsed().as_millis();

        match resp {
            Ok(r) => {
                let raw = r.text;
                let prose = spec.extract(&raw);
                let words = prose.split_whitespace().count();
                let raw_start: String = raw.chars().take(12).collect::<String>().replace('\n', " ");
                let snippet: String = prose
                    .chars()
                    .take(75)
                    .collect::<String>()
                    .replace('\n', " ");
                println!(
                    "{:<32} | {:<7} | {:<6} | {:<12} | {}",
                    name, words, ms, raw_start, snippet
                );
            }
            Err(e) => {
                println!(
                    "{:<32} | ERROR   | {:<6} | {:<12} | Error: {e}",
                    name, ms, "FAIL"
                );
            }
        }
    }

    let g_flawed = "root ::= \"<write>\" text \"</write>\";\ntext ::= char { char };\nchar ::= #'[ -~\\t\\n]';\n";
    let g_fixed_minlen = "root ::= \"<write>\" char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char char { char } \"</write>\";\nchar ::= #'[ -~\\t\\n]';\n";
    let g_fixed_line =
        "root ::= \"<write>\\n\" line { line } \"</write>\";\nline ::= #'[^\\n]+\\n';\n";

    let grammar_tests = vec![
        ("Original Flawed BNF", g_flawed),
        ("Fixed BNF (50-char min length)", g_fixed_minlen),
        ("Fixed BNF (Line-based non-empty lines)", g_fixed_line),
    ];

    println!("\n─── Testing Grammar Formulation Fixes on bnf + nostate + nothink ───");
    println!(
        "{:<38} | {:<7} | {:<6} | {:<12} | {:<80}",
        "Grammar Variant", "Words", "Ms", "Raw Start", "Prose Snippet"
    );
    println!("{}", "─".repeat(145));

    for (g_name, g_str) in grammar_tests {
        let prompt = spec_direct.build_prompt(OUTLINE, WIKI, TASK);
        let _ = backend
            .feed_eos(Some("matrix-grammar-test".to_string()))
            .await;

        let start = Instant::now();
        let resp = backend
            .complete(CompletionRequest {
                system: String::new(),
                prompt,
                grammar: Some(g_str.to_string()),
                temperature: 0.8,
                max_tokens: 300,
                session: Some("matrix-grammar-test".to_string()),
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
        let ms = start.elapsed().as_millis();

        match resp {
            Ok(r) => {
                let raw = r.text;
                let prose = spec_direct.extract(&raw);
                let words = prose.split_whitespace().count();
                let raw_start: String = raw.chars().take(12).collect::<String>().replace('\n', " ");
                let snippet: String = prose
                    .chars()
                    .take(75)
                    .collect::<String>()
                    .replace('\n', " ");
                println!(
                    "{:<38} | {:<7} | {:<6} | {:<12} | {}",
                    g_name, words, ms, raw_start, snippet
                );
            }
            Err(e) => {
                println!(
                    "{:<38} | ERROR   | {:<6} | {:<12} | Error: {e}",
                    g_name, ms, "FAIL"
                );
            }
        }
    }
}

async fn bake(backend: &RemoteBackend, spec: &FormatSpec, session: &str) {
    let (u1, a1) = bake_pair(spec);
    let (u2, a2) = bake_pair_2(spec);
    let _ = backend.feed_eos(Some(session.to_string())).await;
    let _ = backend
        .bake_state(
            session,
            "You write fiction prose.",
            &[(u1.as_str(), a1.as_str()), (u2.as_str(), a2.as_str())],
        )
        .await;
}

fn bake_pair(spec: &FormatSpec) -> (String, String) {
    let outline = "Chapter 1: The Forge\n- Bren works late in the smithy.\n- A mysterious vibration pulses through the floor.";
    let wiki = "Setting: Oakhaven, medieval mountain village.\nBren: village blacksmith, age 32.";
    let task = "Write Chapter 1 based on the outline and world wiki.";
    let user = spec.build_prompt(outline, wiki, task);

    let prose_sample = "Bren wiped soot from his brow, his heavy leather apron stiff with sweat. The forge had been cold for hours, yet a low, rhythmic vibration vibrated through the stone floor beneath his boots. He set down his tongs, resting a gloved hand against the dark iron anvil. The metal was cool to the touch, but the hum grew louder—a steady, deliberate pulse like a hidden heartbeat buried beneath the ancient foundation of the shop.";

    let assistant = match spec {
        FormatSpec::Xml { think: true } => {
            format!("midBren felt the floor vibration.end\n<write>{prose_sample}</write>")
        }
        FormatSpec::Xml { think: false } => {
            format!("<write>{prose_sample}</write>")
        }
        FormatSpec::Marker { think: true } => {
            format!("THINK:Set forge atmosphere.\nWRITE:{prose_sample}")
        }
        FormatSpec::Marker { think: false } => {
            format!("WRITE:{prose_sample}")
        }
        FormatSpec::Separator { think: true } => {
            format!("---THINK---\nSet forge atmosphere.\n---WRITE---\n{prose_sample}\n---END---")
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
    let outline = "Chapter 2: Telemetry\n- Dr. Varma monitors deep space signals.\n- An artificial frequency spike breaks cosmic static.";
    let wiki = "Setting: Deep Sky Array, high-altitude observatory.\nVarma: radio astronomer, 11 hours into shift.";
    let task = "Write Chapter 2 based on the outline and world wiki.";
    let user = spec.build_prompt(outline, wiki, task);

    let prose_sample = "Dr. Varma blinked against the blue glare of her monitoring screens. Eleven hours of uniform cosmic static had lulled the observatory into a quiet stupor. Then, without warning, the frequency analyzer spiked across three independent telemetry bands. The green waveforms coalesced into a tight, deliberate sequence that defied natural stellar phenomena.";

    let assistant = match spec {
        FormatSpec::Xml { think: true } => {
            format!("midDr. Varma analyzes signal spike.end\n<write>{prose_sample}</write>")
        }
        FormatSpec::Xml { think: false } => {
            format!("<write>{prose_sample}</write>")
        }
        FormatSpec::Marker { think: true } => {
            format!("THINK:Describe signal spike.\nWRITE:{prose_sample}")
        }
        FormatSpec::Marker { think: false } => {
            format!("WRITE:{prose_sample}")
        }
        FormatSpec::Separator { think: true } => {
            format!("---THINK---\nDescribe signal spike.\n---WRITE---\n{prose_sample}\n---END---")
        }
        FormatSpec::Separator { think: false } => {
            format!("---CHAPTER---\n{prose_sample}\n---END---")
        }
        FormatSpec::Bracket => {
            format!("[THINK:Signal spike]\n[WRITE:{prose_sample}]")
        }
        FormatSpec::Prose => prose_sample.to_string(),
    };
    (user, assistant)
}
