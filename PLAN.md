# RoCo AI — Plan

## Where we are

✅ **Phase 0–3 complete** — Cargo workspace with all crates, compiles cleanly, 80 tests pass:

```
roco_ai/
├── crates/core/         ← roco_core  (engine, agent, memory, tools, vector, trace, ...)
├── crates/cli/          ← roco binary (demo harness, viz, eval, trace, run-input)
├── crates/session/      ← roco_session (Engine, message queue, poll loop)
├── crates/workspace/    ← roco_workspace (managed filesystem)
├── crates/napi/         ← roco_napi .node addon (napi-rs bindings)
├── crates/gateway/      ← axum HTTP gateway (POST /rpc, SSE streaming)
├── web/app/             ← Next.js 15 + oRPC v1.14 + React 19
├── gui/                 ← Dioxus visualizer (excluded from workspace — GTK deps)
├── SPEC.md              ← architecture spec
├── PROGRESS.md          ← phase tracker
└── PLAN.md              ← this file
```

## The debug loop (the whole point)

A **structured, replayable execution trace** is the primary artifact of every run:

```
Orchestrator → CollectingTracer → Trace → saved to .roco/traces/
                                           → CLI: roco viz / trace list / trace diff
                                           → GUI: Dioxus visualizer
                                           → Web: Next.js app + oRPC → gateway or CLI
                                           → Gateway: axum HTTP + SSE streaming
```

| Before | After |
|--------|-------|
| Playwright screenshot + pass/fail | Structured trace with events, messages, graph |
| Guess which subtask failed | See exact verification failure in trace viewer |
| Re-run to debug | Replay saved trace, diff against new run |
| Unit tests only | Trace becomes regression fixture |

## Running the stack

```bash
# 1. CLI / demos
cargo run -p roco-cli           # demos A–F
cargo run -p roco-cli -- viz    # HTML trace + JSON artifact
cargo run -p roco-cli -- trace list
cargo run -p roco-cli -- trace diff <id1> <id2>
cargo run -p roco-cli -- run-input <file.json>

# 2. Gateway (optional — faster web development)
cargo run -p roco-gateway       # listens on 0.0.0.0:3001

# 3. Web app (requires gateway or CLI fallback)
cd web/app
pnpm install
pnpm dev                       # http://localhost:3000

# 4. napi addon (for direct Node.js integration)
cd crates/napi
napi build --release
```

## Next up: Phase 4 — Real model

Swap `MockBackend` for a real RWKV/SSM backend:

1. Integrate `web-rwkv` crate through the existing `ModelBackend` trait
2. Keep the mock path for tests, evals, and CI
3. Measure quality delta between mock vs real model on the eval suite
