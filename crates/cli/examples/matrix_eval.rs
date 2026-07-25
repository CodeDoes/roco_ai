//! Matrix evaluation example — tests 8-way combinations of:
//!   - BNF vs No-BNF
//!   - State (Baked) vs No-State (Fresh)
//!   - Think vs No-Think
//!
//! Run with:
//!   cargo run --release --example matrix_eval -p roco-cli --features net

use roco_engine::{CompletionRequest, ModelBackend};
use roco_infer_client::RemoteBackend;
use roco_message::FormatSpec;
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

const TASK: &str = "Write Chapter 1 based on the outline and world wiki. Write vivid fiction prose.";

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

    // 1. Prepare Baked Session for Think and Direct
    let session_think_baked = "matrix-xml-think-baked";
    let session_direct_baked = "matrix-xml-direct-baked";

    println!("─── Baking Sessions ───");
    bake(&backend, &spec_think, session_think_baked).await;
    bake(&backend, &spec_direct, session_direct_baked).await;
    println!("✓ Sessions baked.\n");

    let combinations = vec![
        ("1. nobnf + nostate + nothink", &spec_direct, "matrix-fresh-1", false, false, false),
        ("2. nobnf + nostate + think",   &spec_think,  "matrix-fresh-2", false, false, true),
        ("3. nobnf + state   + nothink", &spec_direct, session_direct_baked, false, true, false),
        ("4. nobnf + state   + think",   &spec_think,  session_think_baked,  false, true, true),
        ("5. bnf   + nostate + nothink", &spec_direct, "matrix-fresh-5", true,  false, false),
        ("6. bnf   + nostate + think",   &spec_think,  "matrix-fresh-6", true,  false, true),
        ("7. bnf   + state   + nothink", &spec_direct, session_direct_baked, true,  true, false),
        ("8. bnf   + state   + think",   &spec_think,  session_think_baked,  true,  true, true),
    ];

    println!("{:<32} | {:<7} | {:<6} | {:<12} | {:<80}", "Combination", "Words", "Ms", "Raw Start", "Prose Snippet");
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
                max_tokens: 400,
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
                let snippet: String = prose.chars().take(75).collect::<String>().replace('\n', " ");
                println!(
                    "{:<32} | {:<7} | {:<6} | {:<12} | {}",
                    name, words, ms, raw_start, snippet
                );
            }
            Err(e) => {
                println!("{:<32} | ERROR   | {:<6} | {:<12} | Error: {e}", name, ms, "FAIL");
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
    let prompt = spec.build_prompt(
        "Chapter 1: The Forge\n- Bren works late.",
        "Bren: village blacksmith.",
        "Write Chapter 1.",
    );
    let prose = "Bren wiped soot from his brow. The forge had been cold for hours, yet a low hum vibrated beneath the anvil.";
    let ans = match spec {
        FormatSpec::Xml { think: true } => format!("midBren felt the vibration.end\n<write>{prose}</write>"),
        FormatSpec::Xml { think: false } => format!("<write>{prose}</write>"),
        _ => prose.to_string(),
    };
    (prompt, ans)
}

fn bake_pair_2(spec: &FormatSpec) -> (String, String) {
    let prompt = spec.build_prompt(
        "Chapter 2: Telemetry\n- Varma scans space.",
        "Varma: radio astronomer.",
        "Write Chapter 2.",
    );
    let prose = "Dr. Varma blinked against the blue glare of her monitoring screens. The green waveforms coalesced into a signal.";
    let ans = match spec {
        FormatSpec::Xml { think: true } => format!("midVarma noticed the signal.end\n<write>{prose}</write>"),
        FormatSpec::Xml { think: false } => format!("<write>{prose}</write>"),
        _ => prose.to_string(),
    };
    (prompt, ans)
}
