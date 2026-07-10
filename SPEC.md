# RoCo AI — Architecture Spec

_Status: proposal / foundation._  This document defines the target architecture
so the project can be split into well-bounded parts and debugged without
relying solely on Playwright + unit tests.

## 1. Motivation

Debugging an autonomous agent is painful when the only observability is
end-to-end tests (Playwright) and unit tests. Those tell you *pass/fail* but
not *why*: which subtask failed verification, what the policy gate decided, how
the context budget was spent, where a retry cascade escalated.

The fix is to make a **structured, replayable execution trace** the primary
artifact of every run. The Rust core already has the seed for this
(`crates/core/src/trace.rs`: `TraceEvent` + `Tracer` + `CollectingTracer`). Every future
crate emits into that same trace contract, and the frontend (Dioxus/React)
replays it. A trace is far easier to debug than a screenshot, and it doubles
as the spec for the system's behavior.

## 2. Principles

1. **Pure-Rust core.** All agent logic lives in Rust crates, no UI code mixed in.
2. **One trace contract.** `TraceEvent` is the universal observability type.
   Anything interesting records an event.
3. **Clear boundaries.** Each crate has one job and a typed public API.
4. **Native, not WASM.** The RWKV model (`web-rwkv`) is a **napi-rs** native
   addon; the core and GUI follow the same native path. WASM is optional and
   slower, and `tokio`'s `mio` does not build for WASM.
5. **Typed RPC to the frontend.** The web layer uses **oRPC**; procedures call
   into Rust via **napi-rs**, giving Rust↔TS types end-to-end.

## 3. Monorepo layout

```
roco_ai/                      # cargo workspace (Rust)
├── Cargo.toml                # [workspace] members + shared deps
├── crates/
│   ├── core/                 # the brain (lib) — orchestration, engine, trace,
│   │                         #   capacity, tools, memory, vector, audio, eval
│   ├── cli/                  # `roco` binary — local dev, debugging, `viz`
│   ├── gateway/              # axum HTTP gateway (optional remote/RPC boundary)
│   ├── session/              # session-manager (conversation/event state)
│   ├── workspace/            # workspace-manager (per-task file/artifact state)
│   └── napi/                 # napi-rs bindings → roco_core .node addon
├── web/                      # pnpm workspace (TypeScript)
│   ├── app/                  # Next.js + oRPC server + visualizer UI
│   └── client/               # shared oRPC client / types
├── SPEC.md
└── PROGRESS.md
```

`core` is the single source of truth. `cli`, `gateway`, and `napi` are all
*thin* wrappers that drive `core` and record into the same `trace` stream.

## 4. Crate responsibilities

| Crate | Job | Key API (sketch) |
|-------|-----|------------------|
| `core` | Orchestrator-Worker, engine (RWKV/SSM/RNN), capacity routing, tools, memory, vector RAG, the `trace` contract. | `Orchestrator::run`, `CollectorTracer`, `CapacityPool::select` |
| `cli` | Local harness: run demos, `viz` (emit trace), smoke tests, replay a saved trace. | `roco run --task …`, `roco viz` |
| `gateway` | Optional `axum` HTTP server exposing an RPC endpoint (SSE/WS) that streams `TraceEvent`s to remote clients. | `POST /rpc`, `GET /trace/:id/stream` |
| `session` | Manages a conversation/session: message log, event log, lifecycle. | `Session::push`, `Session::events` |
| `workspace` | Per-task filesystem/artifact state (sandboxes, scratch, outputs). | `Workspace::temp`, `Workspace::snapshot` |
| `napi` | Compiles `core` to a `.node` addon; exposes safe async functions to Node. | `runTask(req) -> Trace`, `streamTrace(id) -> AsyncIterator<TraceEvent>` |

`session` and `workspace` already exist as modules inside the current blob
(`crates/session`, `crates/workspace`) and become first-class crates here.

## 5. Frontend integration (oRPC + napi-rs)

```
Next.js (TypeScript)
  └─ oRPC router  (/api/trpc or app router procedures)
       └─ calls roco_napi .node addon        ← native, no WASM
            └─ roco_core (orchestrator + tracer)
                 └─ web-rwkv (.node, napi-rs) ← model inference
```

- **oRPC server endpoint** lives in Next.js (TS). A procedure like
  `agent.run` calls the napi addon and returns/streams the `TraceEvent` stream.
- The **visualizer** (Dioxus or React) subscribes to that stream and renders
  the four scenes (Stateful Core, Fan-out, ContextBudget, CapacityPool) — the
  same ones already prototyped.
- **Why napi-rs over a Rust HTTP gateway:** it matches `web-rwkv`, avoids a
  second process, and keeps the tight model↔orchestrator loop native/fast.
  The `gateway` crate remains available for remote/multi-client use.

## 6. The debug loop (the whole point)

1. `cli viz` (or the napi `runTask`) produces a `Trace` (events + messages +
   memory graph) — already implemented in `core` + `visualizer`.
2. The trace is saved (e.g. `.roco/traces/<id>.json`) and **replayable**.
3. A failing Playwright/integration test can dump its trace id; you open it in
   the visualizer and see *exactly* where verification failed / budget blew /
   the gate escalated — instead of guessing from a screenshot.
4. Traces become regression fixtures: re-run an old trace's task, diff events.

This replaces "screenshot + pass/fail" with "structured, diffable execution
history" — the real observability the project is missing.

## 7. Phasing

- **Phase 0 — spec + workspace.** This doc; convert the blob into the cargo
  workspace with `core` seeded from current `src/*`, plus `cli`/`session`/
  `workspace` crates. (No behavior change yet.)
- **Phase 1 — trace everywhere.** Ensure `core` records rich events on every
  path; `cli viz` emits them; GUI renders them.
- **Phase 2 — napi + oRPC.** Add `crates/napi`; stand up `web/app` (Next.js +
  oRPC) calling the addon; wire the visualizer to the oRPC stream.
- **Phase 3 — gateway (optional).** `crates/gateway` (axum) for remote access;
  oRPC can proxy to it.
- **Phase 4 — real model.** Swap `MockBackend` for `web-rwkv` via the napi
  bridge; keep the mock path for tests/traces.

## 8. Open decisions (recommended defaults)

- **Transport:** napi-rs primary, axum `gateway` optional. _(Recommended.)_
- **Visualizer UI:** Dioxus (pure Rust, already prototyped) or React inside
  Next.js. _(Either; Dioxus keeps it Rust-only, React shares the Next.js tree.)_
- **Session/workspace** promoted from existing modules verbatim first, then
  thinned.
