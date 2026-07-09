# RoCo AI — Implementation Progress

Tracks work against SPEC.md phases.

## Phase 0 — Cargo workspace + core extraction (IN PROGRESS)

Goal: Convert the single-crate blob into a cargo workspace. No behavior change.

- [x] `SPEC.md` written and committed
- [ ] Convert root `Cargo.toml` to `[workspace]` with members
- [ ] Move `src/*` → `crates/core/src/` (all modules → core library)
- [ ] Create `crates/cli/` — thin binary that drives `roco_core`
- [ ] Create `crates/session/` — session-manager crate (extract `src/session.rs`)
- [ ] Create `crates/workspace/` — workspace-manager crate (extract `src/workspace.rs`)
- [ ] `cargo build --workspace` compiles cleanly
- [ ] `cargo run -p roco-cli` runs demos A-F identically to before

## Phase 1 — Trace everywhere

- [ ] Ensure every crate path records `TraceEvent`s
- [ ] `cli viz` emits traces; GUI renders them
- [ ] Trace replay / diff support

## Phase 2 — napi-rs + oRPC

- [ ] `crates/napi/` — napi-rs bindings to `roco_core`
- [ ] `web/app/` — Next.js + oRPC server calling the addon
- [ ] Visualizer wired to oRPC stream

## Phase 3 — Gateway (optional)

- [ ] `crates/gateway/` — axum HTTP server for remote access
- [ ] oRPC can proxy to gateway

## Phase 4 — Real model

- [ ] Swap `MockBackend` for `web-rwkv` via napi bridge
- [ ] Keep mock path for tests/traces
