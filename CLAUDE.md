# CLAUDE.md — RoCo AI

> Guidance for AI agents (and humans) working in this repo.

## What this is

RoCo AI is a Rust-based agent framework exploring **RNN/RWKV/SSM** architectures for
agentic behavior, context management, and full GPU utilization. The defining idea is
that a **structured, replayable execution trace** is the primary artifact of every
run — replacing "screenshot + pass/fail" with diffable execution history.

## Layout

```
roco_ai/
├── Cargo.toml              # workspace: core/cli/session/workspace/napi/gateway + gui
├── crates/
│   ├── core/            # roco_core — engine, agent, memory, tools, vector, trace, ...
│   ├── cli/             # roco binary: demos A–F, viz, eval, trace, run-input
│   ├── session/         # roco_session: Engine (message queue + poll loop)
│   ├── workspace/       # roco_workspace: managed filesystem
│   ├── napi/            # roco_napi: napi-rs .node addon (cdylib)
│   └── gateway/         # roco-gateway: axum HTTP server (POST /rpc, SSE)
├── web/app/             # Next.js 15 + oRPC v1.14 + React 19 (trace visualizer)
├── gui/                 # Dioxus desktop visualizer (EXCLUDED from workspace — GTK deps)
├── model/               # backend configs (orchestrator/worker/verifier_config)
├── provider/            # adapter descriptors (nvidia/kilo/local_rwkv/mock)
├── agents/              # agent role scaffolding (orchestrator/worker/verifier)
├── evals/              # eval suite definitions (delegation, tool-calls, sandbox, ...)
├── SPEC.md             # architecture spec
├── PLAN.md             # phase plan + run instructions
├── PROGRESS.md         # phase tracker (what's done)
└── README.md           # overview + quick start
```

## Key invariants

1. **Pure-Rust core.** All agent logic lives in `crates/core`; no UI code
   mixed in. Front-ends (`cli`, `gateway`, `napi`, `gui`) are thin wrappers.
2. **One trace contract.** `TraceEvent` (`crates/core/src/trace.rs`) is the
   universal observability type. Anything interesting records an event.
3. **Native, not WASM.** RWKV (`web-rwkv`) is a napi-rs native addon; the
   core + GUI follow the same native path.
4. **Typed RPC to frontend.** Web layer uses **oRPC**; procedures call into
   Rust via either the **gateway** (HTTP, preferred) or the **CLI exec bridge**
   (fallback).

## Model backends (`crates/core/src/engine.rs`, `backends.rs`)

`ModelBackend` is the seam. Current implementations:
- `MockBackend` — deterministic, schema-shaped JSON (used in all tests).
- `NvidiaBackend` / `KiloBackend` — OpenAI-compatible HTTP (gated behind
  `http-backends` cargo feature; needs API keys in env).
- `LocalRwkvBackend` — **Phase 4 placeholder**; returns an error until
  `web-rwkv` is wired (see `provider/local_rwkv_adapter`).

Select via `Config` (`crates/core/src/config.rs`) from `model/*_config`.

## Build & test

```bash
# Rust (exclude gui — needs GTK system deps)
cargo build --workspace --exclude roco-gui
cargo test  --workspace --exclude roco-gui      # 80 tests, 0 fail

# CLI demos
cargo run -p roco-cli            # demos A–F
cargo run -p roco-cli -- viz           # → .roco/traces/roco_trace.{html,json}
cargo run -p roco-cli -- trace list
cargo run -p roco-cli -- trace diff <id1> <id2>

# Gateway (listens on :3001)
cargo run -p roco-gateway

# Web app (needs pnpm install first)
cd web/app && pnpm install && pnpm dev     # http://localhost:3000
```

See `Makefile` for the full target list.

## Web app conventions

- **Path alias:** `@/*` → `src/*` (see `web/app/tsconfig.json`).
- **API routes:** `src/app/api/*` — Next.js App Router route handlers.
  - `POST /api/run-task` — CLI exec bridge (or gateway proxy)
  - `GET  /api/traces` — list saved traces
  - `GET  /api/traces/[id]` — load one trace
  - `POST /api/orpc` — oRPC handler
- **oRPC router:** `src/lib/orpc.ts` — `runTask`, `listTraces`, `loadTrace`,
  `diffTraces`. Auto-detects gateway via `/health`; falls back to CLI/file.
- **TS types:** `src/lib/types.ts` — mirror Rust `Trace`/`TraceEvent`/`TraceSummary`.
- **Visualizer:** `src/components/Visualizer.tsx`.

⚠️ **pnpm gotcha:** pnpm 11 defaults to the `isolated` node-linker, which
leaves `node_modules/` empty (only `.pnpm` store). `web/app/.npmrc` forces
`node-linker=hoisted` so top-level symlinks + `.bin/next` are created.

## Phase status (see PROGRESS.md for detail)

- Phase 0 ✅ — Cargo workspace + core extraction
- Phase 1 ✅ — Trace everywhere (record + persist + diff)
- Phase 2 ✅ — napi-rs bindings + oRPC + web visualizer
- Phase 3 ✅ — Gateway (axum) + oRPC→gateway proxy (verified working)
- Phase 4 ⬜ — Real model (swap `MockBackend` for `web-rwkv` via napi)

## Where to add code

| Want to add... | Put it in... |
|---|---|
| A new agent capability | `crates/core/src/agent.rs` |
| A new tool | `crates/core/src/tools.rs` + `builtins.rs` |
| A new verifier | `crates/core/src/agent.rs` (`Verifier` trait) |
| A new backend | `crates/core/src/backends.rs` + `engine.rs` |
| A new trace event | record via `Tracer` in the relevant module |
| A new web page | `web/app/src/app/<route>/page.tsx` |
| A new oRPC procedure | `web/app/src/lib/orpc.ts` |
