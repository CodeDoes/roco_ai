//! RoCo AI — visualizer (Rust-only foundation).
//!
//! The agent core is instrumented via [`crate::trace`]; this module turns a
//! recorded run into artifacts a viewer can render:
//!
//! * [`Visualizer::render`]        — the original HTML trace (chat + events + graph).
//! * [`Visualizer::render_trace`]  — same, fed from a structured [`TraceEvent`] log.
//! * [`Visualizer::write_json`]    — a structured JSON trace, the stable contract
//!                                   for the web frontend built later.
//!
//! Today the HTML is hand-rolled (Tailwind + vis-network via CDN). The JSON
//! output is the durable part: a future frontend should consume *that*, not the
//! HTML, so the rendering can be swapped without touching the agent.

use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::trace::TraceEvent;

pub struct Visualizer;

impl Visualizer {
    /// Render a trace from already-built `messages`, `events` (strings) and a
    /// `memory_state` knowledge graph (array of `[subject, predicate, object]`).
    pub fn render(
        messages: &Value,
        events: &[String],
        memory_state: &Value,
        output_path: &Path,
    ) -> anyhow::Result<()> {
        let messages_json = serde_json::to_string(messages)?;
        let events_json = serde_json::to_string(events)?;
        let memory_json = serde_json::to_string(memory_state)?;
        let html = build_html(&messages_json, &events_json, &memory_json);
        fs::write(output_path, html)?;
        Ok(())
    }

    /// Render from a structured [`TraceEvent`] log. Events are flattened to the
    /// same string form the HTML template expects, preserving phase/actor/detail.
    pub fn render_trace(
        trace: &[TraceEvent],
        messages: &Value,
        memory_state: &Value,
        output_path: &Path,
    ) -> anyhow::Result<()> {
        let events: Vec<String> = trace
            .iter()
            .map(|e| format!("[{}] {} :: {} — {}", e.ts_ms, e.actor, e.phase, e.detail))
            .collect();
        Self::render(messages, &events, memory_state, output_path)
    }

    /// Emit the durable structured trace. This is the contract the future web
    /// frontend should target: `messages` (the conversation), `events` (the
    /// recorded [`TraceEvent`] stream), and `memory` (the knowledge graph).
    pub fn write_json(
        trace: &[TraceEvent],
        messages: &Value,
        memory_state: &Value,
        output_path: &Path,
    ) -> anyhow::Result<()> {
        let obj = serde_json::json!({
            "schema_version": 1,
            "generator": "roco_ai::visualizer",
            "messages": messages,
            "events": trace,
            "memory": memory_state,
        });
        fs::write(output_path, serde_json::to_string_pretty(&obj)?)?;
        Ok(())
    }
}

/// Build the standalone HTML trace. Kept verbatim from the original foundation;
/// the only inputs are the three JSON-encoded blobs (escaped by `serde`).
fn build_html(messages_json: &str, events_json: &str, memory_json: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>RoCo AI - Agent Trace</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script src="https://cdn.jsdelivr.net/npm/vis-network@current/dist/vis-network.min.js"></script>
    <style>
        body {{ background-color: #0f172a; color: #e2e8f0; font-family: 'Inter', sans-serif; }}
        .chat-bubble {{ max-width: 80%; border-radius: 1rem; padding: 0.75rem 1rem; margin-bottom: 1rem; }}
        .user-bubble {{ background-color: #1e293b; align-self: flex-end; border-bottom-right-radius: 0; }}
        .ai-bubble {{ background-color: #334155; align-self: flex-start; border-bottom-left-radius: 0; }}
        .sidebar {{ background-color: #1e293b; border-left: 1px solid #334155; }}
        .event-node {{ font-family: 'Fira Code', monospace; font-size: 0.8rem; color: #94a3b8; }}
        #memory-graph {{ height: 400px; background: #0f172a; border-radius: 0.5rem; }}
    </style>
</head>
<body class="flex h-screen overflow-hidden">
    <!-- Main Chat Area -->
    <div class="flex-1 flex flex-col h-full">
        <header class="p-4 border-b border-slate-700 flex justify-between items-center bg-slate-900">
            <h1 class="text-xl font-bold text-blue-400">RoCo AI Trace</h1>
            <div class="text-xs text-slate-400">Session: DX-DEMO-01</div>
        </header>

        <div id="chat-container" class="flex-1 overflow-y-auto p-6 flex flex-col">
            <!-- Messages injected here -->
        </div>
    </div>

    <!-- System Sidebar -->
    <div class="w-1/3 sidebar flex flex-col h-full overflow-y-auto p-6 gap-6">
        <section>
            <h2 class="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-4">Agent Identity & State</h2>
            <div id="memory-state" class="bg-slate-800 p-4 rounded-lg text-sm border border-slate-700 whitespace-pre-wrap font-mono">
                <!-- State injected here -->
            </div>
        </section>

        <section>
            <h2 class="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-4">Knowledge Graph</h2>
            <div id="memory-graph"></div>
        </section>

        <section>
            <h2 class="text-sm font-semibold uppercase tracking-wider text-slate-500 mb-4">Execution Trace</h2>
            <div id="event-log" class="space-y-2">
                <!-- Events injected here -->
            </div>
        </section>
    </div>

    <script>
        const messages = {messages_json};
        const events = {events_json};
        const memory = {memory_json};

        // Render Chat
        const container = document.getElementById('chat-container');
        messages.forEach(msg => {{
            const div = document.createElement('div');
            div.className = `chat-bubble ${{msg.role === 'user' ? 'user-bubble' : 'ai-bubble'}}`;
            div.innerText = msg.content;
            container.appendChild(div);
        }});

        // Render State
        document.getElementById('memory-state').innerText = JSON.stringify(memory, null, 2);

        // Render Events
        const log = document.getElementById('event-log');
        events.forEach(evt => {{
            const div = document.createElement('div');
            div.className = 'event-node p-2 border-l-2 border-blue-500 bg-slate-800/50 rounded';
            div.innerText = evt;
            log.appendChild(div);
        }});

        // Render Graph (Triples)
        const nodes = [];
        const edges = [];

        if (Array.isArray(memory)) {{
            memory.forEach(triple => {{
                if (Array.isArray(triple) && triple.length === 3) {{
                    const [s, p, o] = triple;
                    if (!nodes.find(n => n.id === s)) nodes.push({{id: s, label: s, color: '#60a5fa'}});
                    if (!nodes.find(n => n.id === o)) nodes.push({{id: o, label: o, color: '#fbbf24'}});
                    edges.push({{from: s, to: o, label: p, font: {{size: 10, color: '#94a3b8'}}}});
                }}
            }});
        }}

        const graphContainer = document.getElementById('memory-graph');
        const data = {{ nodes: new vis.DataSet(nodes), edges: new vis.DataSet(edges) }};
        const options = {{
            nodes: {{ shape: 'dot', size: 16 }},
            edges: {{ arrows: 'to', color: '#475569' }},
            physics: {{ enabled: true, stabilization: true }}
        }};
        new vis.Network(graphContainer, data, options);
    </script>
</body>
</html>
)"#,
        messages_json = messages_json,
        events_json = events_json,
        memory_json = memory_json,
    )
}
