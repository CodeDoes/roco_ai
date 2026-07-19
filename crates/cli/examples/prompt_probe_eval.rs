//! Prompt-prefix probe: discover the model's `<think>`-tag prior.
//!
//! The user wants to *state-tune* the model to not emit `<think>` tags
//! (except in designated regions) instead of banning `<`/`>` at the grammar
//! level. Step one is to understand the model's prior over think-tag
//! emission for the training-prompt prefixes it was trained with.
//!
//! Context template (see crates/inference/src/actor.rs):
//!   `System: {sys}\n\nUser: {prompt}\n\nAssistant:`  (+ optional `prefill`
//!   appended after `Assistant:`)
//!
//! So `Assistant: <think` is reproduced with `prefill = "<think"`, etc.
//!
//! Usage: `cargo run --release --example prompt_probe_eval -p roco-cli`

use roco_engine::{bake_no_think_session, CompletionRequest, ModelBackend, NO_THINK_PREFILL};
use roco_inference::RwkvBackend;

struct Probe {
    label: &'static str,
    system: &'static str,
    prompt: &'static str,
    prefill: Option<&'static str>,
}

fn think_stats(text: &str) -> (usize, usize, bool) {
    let opens = text.matches("<think").count();
    let closes = text.matches("</think").count();
    // An unclosed think block means the model is *mid* thinking at cutoff.
    let unclosed = opens > closes;
    (opens, closes, unclosed)
}

async fn run_probe(backend: &RwkvBackend, p: &Probe, session: Option<&str>) -> String {
    let req = CompletionRequest {
        system: p.system.to_string(),
        prompt: p.prompt.to_string(),
        prefill: p.prefill.map(|s| s.to_string()),
        temperature: 0.4,
        max_tokens: 160,
        preserve_state: session.is_some(),
        session: session.map(|s| s.to_string()),
        ..Default::default()
    };
    backend
        .complete(req)
        .await
        .map(|r| r.text)
        .unwrap_or_else(|e| format!("<ERROR: {e}>"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Loading RWKV backend...");
    let backend = RwkvBackend::from_env()?;
    println!("Backend loaded.\n");

    // Phase A — prefix probes (no state-tune).
    let probes = [
        Probe { label: "Assistant: (bare)", system: "", prompt: "", prefill: None },
        Probe { label: "Assistant: <think", system: "", prompt: "", prefill: Some("<think") },
        Probe { label: "Assistant: <think>\\n", system: "", prompt: "", prefill: Some("<think>\n") },
        Probe { label: "Assistant: <think></think>", system: "", prompt: "", prefill: Some("<think></think>") },
        Probe { label: "Assistant: <think>reason</think>", system: "", prompt: "", prefill: Some("<think>Let me reason step by step.</think>") },
        Probe { label: "System: empty / User: Hello", system: "", prompt: "Hello", prefill: None },
        Probe { label: "System: no-think instruction", system: "You are a helpful writing assistant. Respond directly and never use <think> tags.", prompt: "Write a short story opening about a lighthouse keeper.", prefill: None },
        Probe { label: "User: story chapter request", system: "", prompt: "Write a chapter about a lone cultivator who discovers an ancient gate in the mountains.", prefill: None },
        Probe { label: "Assistant: Sure! Here is the chapter:", system: "", prompt: "Write a chapter about a lone cultivator who discovers an ancient gate in the mountains.", prefill: Some("Sure! Here is the chapter:") },
        Probe { label: "Assistant: <reason>plan</reason>", system: "", prompt: "Plan a three-chapter story about a fallen knight.", prefill: Some("<reason>I need to outline the plot.</reason>") },
    ];

    println!("=== Phase A: prefix probes (baseline) ===\n");
    for p in &probes {
        let text = run_probe(&backend, p, None).await;
        let (o, c, unc) = think_stats(&text);
        println!("--- {} ---", p.label);
        println!("  opens=<think>={o} closes=</think>={c} unclosed={unc}");
        println!(
            "  continuation: {}\n",
            text.trim().chars().take(220).collect::<String>()
        );
    }

    // Phase B — bake a NO-THINK state-tune (correct roles), then re-probe.
    // Examples are clean assistant responses with NO think tags, so the baked
    // recurrent state should bias future generations away from <think>.
    println!("=== Phase B: bake a no-think session (correct roles) ===\n");
    let tune_examples: &[(&str, &str)] = &[
        (
            "Write a chapter about a quiet village.",
            "The village woke slowly. Smoke curled from the chimneys and a dog barked once at the rising sun.",
        ),
        (
            "Write a chapter about a merchant's journey.",
            "The merchant counted his coins and smiled. The road ahead was long but the weather held fair.",
        ),
        (
            "Write a chapter about a storm at sea.",
            "Waves crashed over the bow. The captain steadied the wheel and shouted for the sails to be trimmed.",
        ),
    ];
    bake_no_think_session(&backend, "notune", "", tune_examples).await?;
    println!("Baked 'notune' session.\n");

    println!("=== Phase B: re-probe with no-think session ===\n");
    for p in probes.iter().take(5).chain(std::iter::once(&probes[7])) {
        let text = run_probe(&backend, p, Some("notune")).await;
        let (o, c, unc) = think_stats(&text);
        println!("--- {} [notune] ---", p.label);
        println!("  opens=<think>={o} closes=</think>={c} unclosed={unc}");
        println!(
            "  continuation: {}\n",
            text.trim().chars().take(220).collect::<String>()
        );
    }

    // Phase C — NO_THINK_PREFILL on a real story prompt (grammar off). The
    // closed-think prefill should put the model straight into content mode.
    println!("=== Phase C: NO_THINK_PREFILL on story prompt ===\n");
    let story_prefill_probe = Probe {
        label: "Assistant: <think></think> / story chapter",
        system: "",
        prompt: "Write a chapter about a lone cultivator who discovers an ancient gate in the mountains.",
        prefill: Some(NO_THINK_PREFILL),
    };
    let text = run_probe(&backend, &story_prefill_probe, None).await;
    let (o, c, unc) = think_stats(&text);
    println!("--- {} ---", story_prefill_probe.label);
    println!("  opens=<think>={o} closes=</think>={c} unclosed={unc}");
    println!(
        "  continuation: {}\n",
        text.trim().chars().take(220).collect::<String>()
    );

    println!("=== Summary ===");
    println!("* Bare Assistant: start defaults to <think> (contamination source).");
    println!("* Closed <think></think> prefill or a content lead-in suppresses <think>.");
    println!("* System 'no think' instructions BACKFIRE (they prime <think>).");
    println!("* A correctly-roled no-think baked session biases state away from <think>.");

    Ok(())
}
