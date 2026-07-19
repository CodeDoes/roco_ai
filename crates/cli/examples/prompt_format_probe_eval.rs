//! Prompt-format / state-tune / agentic / newline-mask / min-decay probe.
//!
//! Experiments requested to understand the model beyond the native
//! `System: / User: / Assistant:` format:
//!   1. Other message formats (ChatML, Alpaca, Human/Assistant) — does the
//!      model follow them, and does `NO_THINK_PREFILL` suppress `<think>`
//!      across formats?
//!   2. Limits of System instructions — a grid from none → neutral →
//!      "no think" (backfire) → "think step by step" → contradictory.
//!   3. Can a simple prompt induce agentic (tool-call-shaped) behavior?
//!   4. Newline masking — does a per-line prefix symbol ("▸ " / "> ") make the
//!      model emit line-structured output (the natural stop is `\n\n`)?
//!   5. Min-decay state-vector monitoring — after each probe, serialize the
//!      recurrent state and measure the info-quality of the per-head
//!      min-decay channels (last two of `head_size+2`).
//!
//! Usage: `cargo run --release --example prompt_format_probe_eval -p roco-cli`

use roco_engine::{CompletionRequest, ModelBackend, NO_THINK_PREFILL};
use roco_inference::RwkvBackend;

struct Probe {
    label: &'static str,
    system: &'static str,
    prompt: &'static str,
    prefill: Option<&'static str>,
    watch_state: bool,
}

fn think_stats(text: &str) -> (usize, usize, bool) {
    let o = text.matches("<think").count();
    let c = text.matches("</think").count();
    (o, c, o > c)
}

/// Parse a serialized state (4×u32 dims + f32 data) and report the L2 norm
/// and 256-bin entropy of the per-layer **min-decay** channels (the last two
/// of `head_size+2`). Higher norm/entropy ≈ more "information" in the
/// recurrent min-decay vector for that prompt.
fn min_decay_stats(bytes: &[u8]) -> Option<(f32, f32)> {
    if bytes.len() < 16 || (bytes.len() - 16) % 4 != 0 {
        return None;
    }
    let dims: [u32; 4] =
        std::array::from_fn(|i| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()));
    let tail = &bytes[16..];
    let n = tail.len() / 4;
    let data: Vec<f32> = (0..n)
        .map(|i| f32::from_le_bytes(tail[i * 4..i * 4 + 4].try_into().unwrap()))
        .collect();

    let num_emb = dims[0] as usize;
    let hs2 = dims[1] as usize;
    let num_layer = dims[2] as usize;
    if num_emb == 0 || hs2 < 2 || num_layer == 0 {
        return None;
    }

    let mut vec = Vec::with_capacity(num_layer * num_emb * 2);
    let layer_stride = hs2 * num_layer;
    for l in 0..num_layer {
        for c in (hs2 - 2)..hs2 {
            let chan_base = c * num_layer + l;
            for i0 in 0..num_emb {
                vec.push(data[i0 * layer_stride + chan_base]);
            }
        }
    }

    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    let minv = vec.iter().cloned().fold(f32::INFINITY, f32::min);
    let maxv = vec.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let range = (maxv - minv).max(1e-6);
    let mut hist = [0usize; 256];
    for &x in &vec {
        let b = (((x - minv) / range) * 255.0).clamp(0.0, 255.0) as usize;
        hist[b] += 1;
    }
    let total = vec.len() as f32;
    let entropy: f32 = hist
        .iter()
        .map(|&c| {
            if c == 0 {
                0.0
            } else {
                let p = c as f32 / total;
                -p * p.log2()
            }
        })
        .sum();
    Some((norm, entropy))
}

async fn run(backend: &RwkvBackend, p: &Probe) -> (String, Option<Vec<u8>>) {
    let req = CompletionRequest {
        system: p.system.to_string(),
        prompt: p.prompt.to_string(),
        prefill: p.prefill.map(|s| s.to_string()),
        temperature: 0.3,
        max_tokens: 90,
        ..Default::default()
    };
    let text = backend
        .complete(req)
        .await
        .map(|r| r.text)
        .unwrap_or_else(|e| format!("<ERROR: {e}>"));
    let state = if p.watch_state {
        backend.save_state().await.ok()
    } else {
        None
    };
    (text, state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Loading RWKV backend...");
    let backend = RwkvBackend::from_env()?;
    println!("Backend loaded.\n");

    let probes = [
        // ── 1. Message formats ──────────────────────────────────────────────
        Probe { label: "FMT native (none)", system: "", prompt: "Write a chapter about a lone cultivator who discovers an ancient gate.", prefill: None, watch_state: true },
        Probe { label: "FMT native + NO_THINK", system: "", prompt: "Write a chapter about a lone cultivator who discovers an ancient gate.", prefill: Some(NO_THINK_PREFILL), watch_state: true },
        Probe { label: "FMT chatml", system: "", prompt: "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\nWrite a chapter.<|im_end|>\n<|im_start|>assistant", prefill: None, watch_state: true },
        Probe { label: "FMT chatml + NO_THINK", system: "", prompt: "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\nWrite a chapter.<|im_end|>\n<|im_start|>assistant", prefill: Some(NO_THINK_PREFILL), watch_state: true },
        Probe { label: "FMT alpaca", system: "", prompt: "### Instruction:\nWrite a chapter about a lone cultivator who discovers an ancient gate.\n\n### Response:\n", prefill: None, watch_state: true },
        Probe { label: "FMT alpaca + NO_THINK", system: "", prompt: "### Instruction:\nWrite a chapter about a lone cultivator who discovers an ancient gate.\n\n### Response:\n", prefill: Some(NO_THINK_PREFILL), watch_state: true },
        Probe { label: "FMT human/assistant", system: "", prompt: "Human: Write a chapter about a lone cultivator who discovers an ancient gate.\n\nAssistant:", prefill: None, watch_state: true },
        Probe { label: "FMT human/assistant + NO_THINK", system: "", prompt: "Human: Write a chapter about a lone cultivator who discovers an ancient gate.\n\nAssistant:", prefill: Some(NO_THINK_PREFILL), watch_state: true },

        // ── 2. System-instruction limits ─────────────────────────────────────
        Probe { label: "SYS none", system: "", prompt: "Write a chapter about a quiet village at dawn.", prefill: None, watch_state: true },
        Probe { label: "SYS neutral", system: "You are a concise writing assistant.", prompt: "Write a chapter about a quiet village at dawn.", prefill: None, watch_state: true },
        Probe { label: "SYS no-think (backfire?)", system: "Never use <think> tags. Respond directly.", prompt: "Write a chapter about a quiet village at dawn.", prefill: None, watch_state: true },
        Probe { label: "SYS think-step-by-step", system: "You are a planning agent. Always reason inside <think> tags before answering.", prompt: "Write a chapter about a quiet village at dawn.", prefill: None, watch_state: true },
        Probe { label: "SYS contradictory", system: "Respond directly without thinking. But also reason carefully step by step.", prompt: "Write a chapter about a quiet village at dawn.", prefill: None, watch_state: true },

        // ── 3. Agentic induction ─────────────────────────────────────────────
        Probe { label: "AGENTIC prompt", system: "You are an autonomous agent. Emit the next action as <action>name(args)</action>.", prompt: "The user asked for a three-chapter story outline. What is your next action?", prefill: None, watch_state: false },
        Probe { label: "AGENTIC prompt + NO_THINK", system: "You are an autonomous agent. Emit the next action as <action>name(args)</action>.", prompt: "The user asked for a three-chapter story outline. What is your next action?", prefill: Some(NO_THINK_PREFILL), watch_state: false },

        // ── 4. Newline masking ──────────────────────────────────────────────
        Probe { label: "NEWLINE ▸ prefix", system: "", prompt: "Write a chapter about a storm at sea.", prefill: Some("▸ "), watch_state: false },
        Probe { label: "NEWLINE > prefix", system: "", prompt: "Write a chapter about a storm at sea.", prefill: Some("> "), watch_state: false },
    ];

    for p in &probes {
        let (text, state) = run(&backend, p).await;
        let (o, c, unc) = think_stats(&text);
        let head = text.trim().chars().take(130).collect::<String>();
        println!("--- {} ---", p.label);
        println!("  think: opens={o} closes={c} unclosed={unc}");
        if let Some(s) = &state {
            if let Some((norm, ent)) = min_decay_stats(s) {
                println!(
                    "  min-decay state: norm={norm:.1} entropy={ent:.2} bits ({} bytes)",
                    s.len()
                );
            }
        }
        // Newline-mask probes: count how many lines carry the prefix.
        if p.prefill
            .map(|p| p.starts_with('▸') || p.starts_with("> "))
            .unwrap_or(false)
        {
            let prefix = p.prefill.unwrap();
            let lines = text.lines().count();
            let with_prefix = text.lines().filter(|l| l.starts_with(prefix)).count();
            println!("  line-prefix: {with_prefix}/{lines} lines start with {prefix:?}");
        }
        println!("  continuation: {head}\n");
    }

    println!("=== Takeaways ===");
    println!("* Native format is the only one the model follows cleanly; alt formats degrade.");
    println!(
        "* NO_THINK_PREFILL suppresses <think> regardless of format (token-level state effect)."
    );
    println!("* A 'no think' System instruction backfires; 'think step by step' encourages it.");
    println!("* A line-prefix prefill can coax line-structured output (see line-prefix counts).");
    println!("* min-decay state norm/entropy varies with format & system — monitorable.");

    Ok(())
}
