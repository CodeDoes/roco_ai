# RoCo AI — Implementation Progress

Tracks work against SPEC.md phases.

## Phase 0 — Cargo workspace + core extraction ✅ DONE

- [x] `SPEC.md` written and committed
- [x] Convert root `Cargo.toml` to `[workspace]` with members
- [x] Move `src/*` → `crates/core/src/` (all modules → core library)
- [x] Create `crates/cli/` — thin binary that drives `roco_core`
- [x] Create `crates/session/` — session-manager crate (extract `src/session.rs`)
- [x] Create `crates/workspace/` — workspace-manager crate (extract `src/workspace.rs`)
- [x] `cargo build --workspace` compiles cleanly
- [x] `cargo run -p roco-cli` runs demos A-F identically to before
- [x] `gui/` excluded from workspace (GTK system deps)
- [x] `crates/gateway/` stub created

## Phase 1 — Trace everywhere ✅ DONE

- [x] Worker emits: budget_check, model_call, tool_parse, tool_exec, tool_result
- [x] Orchestrator passes tracer to spawned Workers
- [x] Trace struct + TraceSummary for structured persistence
- [x] TraceStore: save, load, list, diff
- [x] `roco trace list` — shows all saved traces with summaries
- [x] `roco trace diff <id1> <id2>` — compares two traces
- [x] `viz` runs produce 37 events (was 16) with full Worker coverage
- [x] Trace replay in Dioxus GUI (gui/src/main.rs renders from live TraceEvent stream)

## Phase 2 — napi-rs + oRPC ✅ DONE

- [x] `crates/napi/` — napi-rs bindings to `roco_core`
  - `runTask()`, `listTraces()`, `loadTrace()`, `diffTraces()` — all return JSON strings
  - Async functions that run the full orchestrator with `CollectingTracer`
  - Auto-saves traces to `.roco/traces/` via `TraceStore`
  - Compiles as `cdylib` (`roco_napi.node`) via napi-rs
- [x] `web/app/` — Next.js 15 + oRPC v1.14 + React 19
  - **API routes**: `POST /api/run-task` (CLI exec bridge), `GET /api/traces`, `GET /api/traces/[id]`, `POST /api/orpc` (oRPC handler)
  - **oRPC router** (`src/lib/orpc.ts`): `runTask`, `listTraces`, `loadTrace`, `diffTraces` procedures
  - **Main page** (`src/app/page.tsx`): Task input form + live trace visualizer
  - **Visualizer component** (`src/components/Visualizer.tsx`): Event timeline, phase-colored rows, expandable metadata, message log, execution summary
  - **TypeScript types** (`src/lib/types.ts`): Mirrors Rust Trace/TraceEvent/TraceSummary structs
  - CLI bridge: executes `cargo run -p roco-cli -- run-input <file>` in dev, `roco run-input` in production
- [x] Visualizer wired to oRPC stream
  - Main page fetches trace from `/api/run-task`, renders via Visualizer component
  - Three tabs: Events (timeline), Messages (chat), Summary (phase counts)
  - Stats bar shows subtasks, failed, model calls, duration, events, tool calls, retries, errors

## Phase 3 — Gateway ✅ DONE (verified working)

- [x] `crates/gateway/` — axum HTTP server for remote access
  - `POST /rpc` — run a task via the orchestrator, returns trace JSON
  - `GET /traces` — list saved traces
  - `GET /trace/:id` — get a full trace by ID
  - `GET /trace/:id/stream` — SSE-stream the events of a saved trace
  - `GET /health` — health check endpoint
  - Runs on `0.0.0.0:3001` by default
  - No warnings, compiles cleanly with workspace deps
- [x] oRPC can proxy to gateway
  - `orpc.ts` auto-detects gateway via `/health` check (2s timeout)
  - When gateway is available: proxies `runTask`, `listTraces`, `loadTrace`, `diffTraces` to it
  - Falls back to CLI exec / direct file reads when gateway is unreachable
  - Controlled by `GATEWAY_URL` (default `http://localhost:3001`) and `PREFER_GATEWAY` env vars
- [x] Gateway verified end-to-end
  - `GET /health` → `{"status":"ok","service":"roco-gateway","version":"0.1.0"}`
  - `GET /traces` → lists saved traces (37 events, 7 subtasks, 0 failed)
  - `POST /rpc` → runs full task, returns structured trace JSON
- [x] Web app **builds cleanly** (`pnpm build` → 9 routes compile, type-check passes)
  - Added `web/app/.npmrc` with `node-linker=hoisted` (pnpm 11 defaulted to
    isolated linker, leaving node_modules empty — fixed by forcing hoisted)

## Phase 4 — Real model

- [ ] Swap `MockBackend` for `web-rwkv` via napi bridge
- [ ] Keep mock path for tests/traces
