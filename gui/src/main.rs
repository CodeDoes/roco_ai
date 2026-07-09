//! RoCo AI — GUI (Dioxus, Rust + RSX + CSS).
//!
//! The UI is written entirely in Rust: each scene is a Dioxus component using
//! RSX (Rust's JSX). The data comes straight from the `roco_ai` library — the
//! GUI runs a real (mock-backed) orchestration via `Orchestrator` +
//! `CollectingTracer` and renders the recorded `TraceEvent`s. No JSON files,
//! no separate server: the core crate *is* the data source.

use dioxus::prelude::*;
use roco_ai::trace::TraceEvent;

const TABS: &[(&str, &str)] = &[
    ("Stateful Core", "O(1) state vs KV-cache growth"),
    ("Fan-out", "orchestrator → workers → verify → aggregate"),
    ("ContextBudget", "hard 4K allocation + 3000 prompt cap"),
    ("CapacityPool", "backend routing by capacity"),
];

// ---- data shapes ---------------------------------------------------------
#[derive(Clone, Debug)]
struct Trace {
    messages: Vec<Msg>,
    events: Vec<TraceEvent>,
    memory: Vec<Vec<String>>,
}
#[derive(Clone, Debug)]
struct Msg {
    role: String,
    content: String,
}

/// Run a real (mock-backed) orchestration through the `roco_ai` library and
/// record its execution trace. This is the single source of truth for the UI.
async fn build_trace() -> Trace {
    use roco_ai::agent::{ChecklistVerifier, ContextBudget, Orchestrator, RetryPolicy, Task};
    use roco_ai::engine::MockBackend;
    use roco_ai::trace::CollectingTracer;
    use std::sync::Arc;

    let backend = Arc::new(MockBackend {
        name: "mock-3b".into(),
        ..Default::default()
    });
    let tracer = CollectingTracer::new();
    let orch = Orchestrator::new(
        backend,
        ContextBudget::default(),
        ChecklistVerifier,
        RetryPolicy::default(),
    )
    .with_tracer(Arc::new(tracer.clone()));

    let context: String = (0..400)
        .map(|i| format!("Fact {}: the orchestrator routes subtask {} through a verification gate. ", i, i))
        .collect();
    let task = Task {
        id: "doc-review".into(),
        objective: "Review the provided facts and summarize.".into(),
        context,
        output_schema: r#"{"result": "<string>"}"#.into(),
        allow_abstain: true,
    };
    let subs = orch.decompose(&task);
    let _ = orch.run(&task).await; // recorded into `tracer` regardless of outcome
    let events = tracer.snapshot();

    let mut memory = vec![vec![
        "orchestrator".into(),
        "decomposed_into".into(),
        format!("{} subtasks", subs.len()),
    ]];
    for s in &subs {
        memory.push(vec!["orchestrator".into(), "spawned".into(), s.id.clone()]);
        memory.push(vec![s.id.clone(), "used_backend".into(), "mock-3b".into()]);
    }
    let messages = vec![
        Msg {
            role: "user".into(),
            content: format!(
                "Objective: {}\n\n(Context chunked into {} atomic 4K subtasks)",
                task.objective, subs.len()
            ),
        },
        Msg {
            role: "assistant".into(),
            content: format!("Aggregated {} subtask outputs via the orchestrator.", subs.len()),
        },
    ];
    Trace { messages, events, memory }
}

/// Illustrative fallback if the orchestration cannot run in this environment.
fn default_trace() -> Trace {
    let n = 6;
    let mut memory = vec![vec![
        "orchestrator".into(),
        "decomposed_into".into(),
        format!("{n} subtasks"),
    ]];
    let mut events = vec![TraceEvent::new(
        "decompose",
        "orchestrator",
        format!("split task into {n} subtasks"),
    )];
    for i in 1..=n {
        memory.push(vec!["orchestrator".into(), "spawned".into(), format!("worker-doc-{i}")]);
        memory.push(vec![format!("worker-doc-{i}"), "used_backend".into(), "mock-3b".into()]);
        events.push(TraceEvent::new(
            "execute",
            format!("worker-doc-{i}"),
            "model call + verify",
        ));
        events.push(TraceEvent::new("verify", "verifier", "gate passed"));
    }
    events.push(TraceEvent::new("aggregate", "aggregator", "merged outputs"));
    Trace { messages: vec![], events, memory }
}

fn worker_count(t: &Trace) -> usize {
    t.memory.iter().filter(|m| m.len() == 3 && m[1] == "spawned").count().max(1)
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut tab = use_signal(|| 0usize);
    let mut playing = use_signal(|| true);
    let mut speed = use_signal(|| 1.0f32);
    let time = use_signal(|| 0.0f64);

    // Animation loop: advance `time` while playing, on a background thread
    // (native desktop build — no WASM timers needed).
    let _ = use_effect(move || {
        if !playing() {
            return;
        }
        let time = time;
        let playing = playing;
        let speed = speed;
        std::thread::spawn(move || {
            let mut time = time;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(16));
                if !playing() {
                    break;
                }
                time.set(time() + 0.016 * speed() as f64);
            }
        });
    });

    // Build the real trace by running the orchestration from the `roco_ai` lib.
    let trace = use_resource(build_trace);
    let trace = trace().cloned().unwrap_or_else(default_trace);

    let t = time();
    let spd = speed();

    rsx! {
        div { id: "main",
            div { class: "hero",
                div { class: "brand",
                    div { class: "logo" }
                    div {
                        h1 { "RoCo " span { class: "grad", "AI" } " — Visualizer" }
                        div { class: "tag", "RNN · RWKV · SSM agents — Rust UI (Dioxus / RSX)" }
                    }
                }
                div { class: "controls",
                    button {
                        class: "btn primary",
                        onclick: move |_| playing.set(!playing()),
                        if playing() { "❚❚ Pause" } else { "▶ Play" }
                    }
                    span { "speed" }
                    input {
                        r#type: "range", min: "0.3", max: "2.5", step: "0.1",
                        value: speed().to_string(),
                        oninput: move |e| speed.set(e.value().parse().unwrap_or(1.0)),
                    }
                    span { "{spd:.1}×" }
                }
            }

            div { class: "tabs",
                for (i, (name, _)) in TABS.iter().enumerate() {
                    div {
                        class: if tab() == i { "tab active" } else { "tab" },
                        onclick: move |_| tab.set(i),
                        "{name}"
                    }
                }
            }

            div { class: "stage",
                { match tab() {
                    0 => rsx! { StatefulScene { time: t } },
                    1 => rsx! { FanoutScene { time: t, workers: worker_count(&trace) } },
                    2 => rsx! { BudgetScene { time: t } },
                    _ => rsx! { CapacityScene { time: t, speed: spd } },
                } }
            }

            div { class: "events",
                for e in trace.events.iter().rev().take(40) {
                    div { class: "e",
                        span { class: "p", "● {e.phase}" }
                        span { "{e.actor}: {e.detail}" }
                    }
                }
            }

            div { class: "footer",
                "Built with Dioxus (Rust + RSX + CSS). Data: "
                code { "cargo run -- viz" }
                " → "
                code { ".roco/traces/roco_trace.json" }
                " → fetched by the app."
            }
        }
    }
}

// --- Scene 1: Stateful Core -------------------------------------------------
#[component]
fn StatefulScene(time: f64) -> Element {
    let tokens = (time * 6.0) as usize;
    let cells = (time * 6.0).min(60.0);
    rsx! {
        h2 { "① Stateful Core — O(1) state vs KV-cache" }
        p { class: "sub",
            "RWKV / SSM collapse history into a fixed-size hidden state every step, so memory stays "
            "constant. A transformer must persist every key/value pair — memory grows with context."
        }
        svg { view_box: "0 0 920 360", height: "360",
            text { x: "290", y: "24", fill: "#38f0ff", "font_size": "14", "text_anchor": "middle",
                "RoCo · Stateful (RWKV / SSM)" }
            text { x: "650", y: "24", fill: "#ff5cc8", "font_size": "14", "text_anchor": "middle",
                "Transformer · KV-Cache" }

            // streaming tokens
            for i in 0..26 {
                StatefulToken { i: i, time: time }
            }

            // O(1) state block (left)
            rect {
                x: "240", y: "200",
                width: { (104.0 * (1.0 + 0.04 * (time * 2.0).sin() as f32)).to_string() },
                height: { (70.0 * (1.0 + 0.04 * (time * 2.0).sin() as f32)).to_string() },
                rx: "10", fill: "#122a33", stroke: "#38f0ff", "stroke_width": "1.5",
            }
            text { x: "290", y: "232", fill: "#dfe7ff", "font_size": "12", "text_anchor": "middle", "STATE" }
            text { x: "290", y: "250", fill: "#38f0ff", "font_size": "12", "text_anchor": "middle", "64 units" }

            // growing KV-cache grid (right)
            for i in 0..(cells as usize) {
                KvCell { i: i }
            }

            text { x: "20", y: "344", fill: "#dfe7ff", "font_size": "12", "font_family": "monospace",
                "tokens streamed: {tokens}" }
            text { x: "20", y: "358", fill: "#8a96c4", "font_size": "11", "font_family": "monospace",
                "RoCo state: 64 units (constant)  •  KV-cache: O(N) → blowup" }
        }
        div { class: "stats",
            Stat { k: "tokens streamed", v: tokens.to_string() }
            Stat { k: "RoCo state", v: "64 units".to_string() }
            Stat { k: "KV-cache", v: format!("{} cells", cells as usize) }
            Stat { k: "state growth", v: "O(1)".to_string() }
        }
    }
}

#[component]
fn StatefulToken(i: usize, time: f64) -> Element {
    let phase = ((time * 0.6 + i as f64 * 0.137) % 1.0) as f32;
    if phase > 1.0 {
        return rsx! { };
    }
    let target = if i % 2 == 0 { 290.0 } else { 650.0 };
    let x = 460.0 + (target - 460.0) * phase as f64;
    let y = 180.0 + (1.0 - (phase as f64 - 0.5).abs() * 2.0) * -22.0;
    let c = if i % 2 == 0 { "#38f0ff" } else { "#ff5cc8" };
    rsx! {
        circle { cx: x.to_string(), cy: y.to_string(), r: "3", fill: c }
    }
}

#[component]
fn KvCell(i: usize) -> Element {
    let cols = 8;
    let xi = i % cols;
    let yi = i / cols;
    let x = 560.0 + xi as f64 * 11.0;
    let y = 200.0 + yi as f64 * 11.0;
    let a = 0.45 + 0.55 * ((i + 1) as f64 / 60.0);
    rsx! {
        rect {
            x: x.to_string(), y: y.to_string(), width: "9", height: "9", rx: "2",
            fill: format!("rgba(255,92,200,{a:.2})"),
        }
    }
}

// --- Scene 2: Fan-out (real worker count) ----------------------------------
#[component]
fn FanoutScene(time: f64, workers: usize) -> Element {
    let n = workers.max(1);
    let work_y: Vec<f32> = (0..n).map(|i| 90.0 + (if n > 1 { 220.0 / (n - 1) as f32 } else { 0.0 }) * i as f32).collect();
    // Precompute pulse-dot positions (RSX `for` bodies must be pure elements).
    let dots: Vec<(f32, f64)> = work_y
        .iter()
        .map(|&wy| {
            let ph = (time * 0.8) % 1.0;
            let x = 130.0 + (430.0 - 130.0) * ph as f64;
            (wy, x)
        })
        .collect();
    rsx! {
        h2 { "② Orchestrator → Worker fan-out + verification" }
        p { class: "sub",
            "A task is decomposed into {n} 4K-chunk subtasks, fanned out to parallel workers (each runs its "
            "own tool-calling loop), vetted by a verification gate, then aggregated. Failed gates trigger a "
            "retry / escalation cascade."
        }
        svg { view_box: "0 0 920 420", height: "420",
            // orchestrator -> workers
            for &wy in work_y.iter() {
                line { x1: "130", y1: "210", x2: "430", y2: wy.to_string(),
                    stroke: "rgba(120,160,255,0.5)", "stroke_width": "1.5" }
            }
            // workers -> verifier
            for (i, &wy) in work_y.iter().enumerate() {
                line { x1: "460", y1: wy.to_string(), x2: "760", y2: "130",
                    stroke: if i == n / 2 { "#ff6b6b" } else { "#38f0ff" }, "stroke_width": "1.3" }
            }
            line { x1: "770", y1: "160", x2: "770", y2: "260", stroke: "#38f0ff", "stroke_width": "1.6" }
            line { x1: "800", y1: "210", x2: "880", y2: "210", stroke: "#46f7a5", "stroke_width": "1.8" }

            // traveling pulse dots
            for (wy, x) in dots {
                circle { cx: x.to_string(), cy: wy.to_string(), r: "2.5", fill: "#38f0ff" }
            }

            // nodes
            Node { x: 100.0, y: 210.0, r: 30.0, fill: "#b07bff", label: "ORCH", sub: "decompose" }
            for (i, &wy) in work_y.iter().enumerate() {
                Node { x: 445.0, y: wy, r: 24.0, fill: "#38f0ff", label: format!("W{}", i + 1), sub: "" }
                Ring { x: 445.0, y: wy, prog: ((time * 0.5 + i as f64 * 0.2) % 1.0) as f32 }
                if i == n / 2 {
                    text { x: "475", y: (wy - 22.0).to_string(), fill: "#ff6b6b", "font_size": "11", "font_family": "monospace", "✕ verify" }
                } else {
                    text { x: "475", y: (wy - 22.0).to_string(), fill: "#46f7a5", "font_size": "11", "font_family": "monospace", "✓ verify" }
                }
            }
            Node { x: 770.0, y: 130.0, r: 28.0, fill: "#ffce5c", label: "VERIFY", sub: "gate" }
            Node { x: 770.0, y: 290.0, r: 28.0, fill: "#38f0ff", label: "AGG", sub: "merge" }
            Node { x: 890.0, y: 210.0, r: 26.0, fill: "#46f7a5", label: "OUT", sub: "final" }
        }
    }
}

#[component]
fn Node(x: f32, y: f32, r: f32, fill: String, label: String, sub: String) -> Element {
    rsx! {
        circle { cx: x.to_string(), cy: y.to_string(), r: r.to_string(),
            fill: format!("rgba({},{},{},0.35)", hex_r(&fill), hex_g(&fill), hex_b(&fill)), stroke: fill, "stroke_width": "1.6" }
        text { x: x.to_string(), y: (y - if sub.is_empty() { 0.0 } else { 3.0 }).to_string(),
            fill: "#dfe7ff", "font_size": "11", "text_anchor": "middle", "font_family": "monospace", "{label}" }
        if !sub.is_empty() {
            text { x: x.to_string(), y: (y + 12.0).to_string(), fill: "#8a96c4", "font_size": "9",
                "text_anchor": "middle", "font_family": "monospace", "{sub}" }
        }
    }
}

#[component]
fn Ring(x: f32, y: f32, prog: f32) -> Element {
    let segs = 24;
    let arcs: Vec<(f32, f32)> = (0..segs)
        .filter(|&s| (s as f32 / segs as f32) <= prog)
        .map(|s| {
            let a0 = -std::f32::consts::FRAC_PI_2 + s as f32 / segs as f32 * std::f32::consts::TAU;
            let a1 = a0 + std::f32::consts::TAU / segs as f32;
            (a0, a1)
        })
        .collect();
    rsx! {
        for (a0, a1) in arcs {
            line {
                x1: (x + 29.0 * a0.cos()).to_string(), y1: (y + 29.0 * a0.sin()).to_string(),
                x2: (x + 29.0 * a1.cos()).to_string(), y2: (y + 29.0 * a1.sin()).to_string(),
                stroke: "#38f0ff", "stroke_width": "3",
            }
        }
    }
}

// --- Scene 3: ContextBudget ------------------------------------------------
#[component]
fn BudgetScene(time: f64) -> Element {
    let segs = [
        ("system", 700usize, "#38f0ff"),
        ("task", 1200, "#b07bff"),
        ("tools", 800, "#ff5cc8"),
        ("scratch", 700, "#ffce5c"),
        ("generation", 1536, "#46f7a5"),
    ];
    let total: usize = segs.iter().map(|(_, t, _)| *t).sum();
    let max_prompt = 3000usize;
    rsx! {
        h2 { "③ ContextBudget — hard 4K allocation" }
        p { class: "sub",
            "The window is split deterministically; the combined prompt is capped at 3000 tokens. The red rule "
            "is the hard limit — fits_prompt() rejects anything over it."
        }
        svg { view_box: "0 0 920 200", height: "200",
            {
                let mut x = 20.0f64;
                let nodes = segs.iter().map(|(name, tok, c)| {
                    let w = *tok as f64 / total as f64 * 880.0;
                    let rx = x;
                    x += w;
                    rsx! {
                        rect { x: rx.to_string(), y: "50", width: w.to_string(), height: "54", fill: c }
                        text { x: (rx + w / 2.0).to_string(), y: "80", fill: "#05060c", "font_size": "11",
                            "text_anchor": "middle", "font_family": "monospace", "{name}\n{tok}" }
                    }
                }).collect::<Vec<_>>();
                rsx! { {nodes.into_iter()} }
            }
            // hard prompt cap line
            {
                let cap_x = 20.0 + max_prompt as f64 / total as f64 * 880.0;
                rsx! {
                    line { x1: cap_x.to_string(), y1: "36", x2: cap_x.to_string(), y2: "118",
                        stroke: "#ff6b6b", "stroke_width": "2" }
                    text { x: cap_x.to_string(), y: "28", fill: "#ff6b6b", "font_size": "11",
                        "text_anchor": "middle", "font_family": "monospace", "max prompt = {max_prompt}" }
                }
            }
            text { x: "20", y: "160", fill: "#8a96c4", "font_size": "12", "font_family": "monospace",
                "total = {total} tokens · prompt hard-capped at {max_prompt}" }
        }
        div { class: "stats",
            Stat { k: "total window", v: format!("{total} tok") }
            Stat { k: "max prompt", v: format!("{max_prompt} tok") }
            Stat { k: "fits_prompt()", v: "true".to_string() }
        }
    }
}

// --- Scene 4: CapacityPool -------------------------------------------------
#[component]
fn CapacityScene(time: f64, speed: f32) -> Element {
    let names = ["LocalRwkv7", "Rwkv7Cpu7B", "Rwkv7Cpu13B", "KiloHy3", "Nvidia"];
    let specs = ["gpu 4gb + cache 4gb", "cpu 1 + ram 15gb", "cpu 1 + ram 27gb", "kilo:1 tencent", "nvidia:1 gpu"];
    let colors = ["#38f0ff", "#b07bff", "#ff5cc8", "#ffce5c", "#46f7a5"];
    // deterministic per-backend "free %" oscillation, staggered
    rsx! {
        h2 { "④ CapacityPool — backend routing by capacity" }
        p { class: "sub",
            "Each backend advertises a Capacity; select(free, order) picks the first that fits. GPU and CPU "
            "pools are independent, so they run concurrently — full utilization."
        }
        svg { view_box: "0 0 920 220", height: "220",
            {
                let mut x = 20.0f64;
                let cards = (0..5).map(|i| {
                    let cw = (920.0 - 40.0 - 14.0 * 4.0) / 5.0;
                    let rx = x;
                    x += cw + 14.0;
                    let free = (0.5 + 0.5 * (time * (0.6 + i as f64 * 0.15) * speed as f64).sin()) * 100.0;
                    let lit = free < 45.0;
                    rsx! {
                        rect { x: rx.to_string(), y: "50", width: cw.to_string(), height: "150",
                            rx: "12",
                            fill: if lit { "#131f33" } else { "#080b16" },
                            stroke: colors[i], "stroke_width": if lit { "1.6" } else { "1" } }
                        text { x: (rx + 10.0).to_string(), y: "70", fill: "#dfe7ff", "font_size": "12", "font_family": "monospace", "{names[i]}" }
                        text { x: (rx + 10.0).to_string(), y: "88", fill: "#8a96c4", "font_size": "10", "font_family": "monospace", "{specs[i]}" }
                        rect { x: (rx + 10.0).to_string(), y: "110", width: (cw - 20.0).to_string(), height: "12", rx: "6", fill: "rgba(255,255,255,0.08)" }
                        rect { x: (rx + 10.0).to_string(), y: "110", width: ((cw - 20.0) * free / 100.0).to_string(), height: "12", rx: "6", fill: colors[i] }
                        text { x: (rx + 10.0).to_string(), y: "140", fill: "#8a96c4", "font_size": "11", "font_family": "monospace", "free {free:.0}%" }
                        if lit {
                            text { x: (rx + cw - 10.0).to_string(), y: "70", fill: colors[i], "font_size": "10", "text_anchor": "end", "font_family": "monospace", "● active" }
                        }
                    }
                }).collect::<Vec<_>>();
                rsx! { {cards.into_iter()} }
            }
            text { x: "20", y: "220", fill: "#8a96c4", "font_size": "12", "font_family": "monospace",
                "select(free, order) routes each subtask to the first backend that fits · GPU+CPU concurrent" }
        }
    }
}

// --- small shared bits -----------------------------------------------------
#[component]
fn Stat(k: String, v: String) -> Element {
    rsx! {
        div { class: "stat",
            div { class: "k", "{k}" }
            div { class: "v", "{v}" }
        }
    }
}

// crude hex -> channel helpers for the rgba() fill trick in Node
fn hex_r(h: &str) -> u8 { u8::from_str_radix(&h[1..3], 16).unwrap_or(0) }
fn hex_g(h: &str) -> u8 { u8::from_str_radix(&h[3..5], 16).unwrap_or(0) }
fn hex_b(h: &str) -> u8 { u8::from_str_radix(&h[5..7], 16).unwrap_or(0) }
