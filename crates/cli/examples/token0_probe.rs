//! Token-0 probe: compare NO_THINK_PREFILL vs EOS-padded state-tuning.
//!
//! Run with a real RWKV model:
//!   RWKV_MODEL=... cargo run --release --example token0_probe -p roco-cli
//!
//! This probe:
//! 1. Bakes a no-think session WITH token-0 EOS padding between examples
//! 2. Generates WITHOUT any NO_THINK_PREFILL at generation time
//! 3. Reports think-tag statistics to compare effectiveness
//!
//! Hypothesis: EOS-padded state-tuning replaces the need for generation-time
//! NO_THINK_PREFILL because token 0 (document separator in training) properly
//! bounds the state between tuning examples.

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

    // Phase A — prefix probes (baseline, no state-tune).
    // Same probes as prompt_probe_eval.rs for direct comparison.
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

    println!("======================================================================");
    println!("Phase A: prefix probes (baseline, no state-tune)");
    println!("======================================================================\n");
    for p in &probes {
        let text = run_probe(&backend, p, None).await;
        let (o, c, unc) = think_stats(&text);
        println!("--- {} ---", p.label);
        println!("  opens=<think>={o} closes=</think>={c} unclosed={unc}");
        println!("  continuation: {}", text.trim().chars().take(220).collect::<String>());
        println!();
    }

    // Phase B — NEW approach: EOS-padded state-tuning, NO generation-time prefill.
    // Uses the bake_no_think_session function which now feeds token 0 (EOS)
    // between examples. The hypothesis is that this EOS padding makes the
    // state match the training distribution, so the baked state alone is
    // sufficient to suppress <think> without any prefill at generation time.
    println!("======================================================================");
    println!("Phase B: EOS-padded state-tuning (NEW approach)");
    println!("  bake_no_think_session now feeds token 0 between examples.");
    println!("  Generation uses NO prefill — pure state-tune.");
    println!("======================================================================\n");

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

    // Bake with EOS padding (new behavior — feed_eos called between examples)
    bake_no_think_session(&backend, "eos_tuned", "", tune_examples).await?;
    println!("Baked 'eos_tuned' session (with EOS padding).\n");

    // Re-probe WITHOUT any NO_THINK_PREFILL — pure state-tune
    println!("--- Re-probe without NO_THINK_PREFILL (EOS-tuned state only) ---\n");
    for p in probes.iter().take(5).chain(std::iter::once(&probes[7])) {
        let text = run_probe(&backend, p, Some("eos_tuned")).await;
        let (o, c, unc) = think_stats(&text);
        println!("--- {} [eos_tuned] ---", p.label);
        println!("  opens=<think>={o} closes=</think>={c} unclosed={unc}");
        println!("  continuation: {}", text.trim().chars().take(220).collect::<String>());
        println!();
    }

    // Phase C — OLD approach: NO_THINK_PREFILL at generation time (for comparison).
    println!("======================================================================");
    println!("Phase C: NO_THINK_PREFILL at generation time (OLD approach)");
    println!("  No state-tune. Relies on <think></think> prefill.");
    println!("======================================================================\n");

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
    println!("  continuation: {}", text.trim().chars().take(220).collect::<String>());
    println!();

    // Phase D — Combined: EOS-padded state-tuning + NO generation-time prefill.
    // The ideal approach: state carries the no-think bias, no prefill needed.
    println!("======================================================================");
    println!("Phase D: Summary — comparing approaches");
    println!("======================================================================");
    println!("* Phase A (baseline): bare Assistant: starts with <think>");
    println!("* Phase B (NEW): EOS-padded state-tune, NO generation prefill");
    println!("* Phase C (OLD): NO_THINK_PREFILL at generation time");
    println!();
    println!("Expected: Phase B >= Phase C effectiveness");
    println!("  Token 0 (EOS) between tuning examples matches training distribution.");
    println!("  See: https://github.com/BlinkDL/RWKV-LM/blob/main/RWKV-v5/make_data.py");
    println!("  'Here \"/\" means end_of_doc, which is actually token [0]'");

    Ok(())
}
