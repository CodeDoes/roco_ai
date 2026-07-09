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
- [ ] Trace replay in GUI / web frontend

## Phase 2 — napi-rs + oRPC (NEXT)

- [ ] `crates/napi/` — napi-rs bindings to `roco_core`
- [ ] `web/app/` — Next.js + oRPC server calling the addon
- [ ] Visualizer wired to oRPC stream

## Phase 3 — Gateway (optional)

- [ ] `crates/gateway/` — axum HTTP server for remote access
- [ ] oRPC can proxy to gateway

## Phase 4 — Real model

- [ ] Swap `MockBackend` for `web-rwkv` via napi bridge
- [ ] Keep mock path for tests/traces
