# PROGRESS.md — Phase 8 Verification Log

Lean delta log — one entry per change, newest first. **How to use this file properly: see AGENTS.md §9 "Using PROGRESS.md".** Canonical state lives in AGENTS.md (§9 loop, §10 known issues, §12 UX, §13 router NLU). Old history is in git.

## 2026-08-01 — All 11 pending tests + story API stubs landed (intent in prior edit of this entry)

- **0 ignored tests left in the workspace** — all 11 are now real and green.
- MockBackend mirrors the real actor: `on_token` emits whitespace-delimited chunks; `deadline_ms` → `TimedOut`; `interrupt()` cancels in-flight latency; `top_a` truncates the BNF-walk candidate set (uniform-mass analog); `bnf_mask` drives deterministic masked generation; intent-classification prompts return score-based keyword intent JSON.
- **`blend_states` generalized to N states**: trait takes `&[(&str, f32)]` (weights normalized), default errors descriptively; RwkvBackend + actor use pure `roco_engine::blend_weighted` (unit-tested: 3-state math, normalization, zero-weight/length-mismatch edges); infer-client + example updated.
- **Story API stubs → real**: `update_outline`/`save_chapter` via new `StoryEngine::set_outline`/`save_chapter` (validating + persisting to `01-OUTLINE.md`/`03-CHAPTER_N.md`); `revise` = evaluate → critique → revise; `suggestions`/`continue` via WritingAssistant (real model calls); `apply_suggestion` = deterministic merge (fill_middle/alternative/continuation) into editor text.
- **Router NLU testable under mock** (§13 item 1a): `test_detect_intent_with_mock_backend` proves story/adventure/html/coder/chat routing.
- `web_rwkv_version` now emitted by `engine-gpu/build.rs` from Cargo.lock (was hardcoded). Fixed pre-existing flake: all env-var-mutating port tests are now serialized behind `PORT_TEST_LOCK` (a static mutex) — save/restore alone still raced.
- `cargo test --workspace`: **1005 passed / 0 failed / 0 ignored**. `cargo check --workspace --all-targets` + `cargo fmt --check` clean, zero warnings.
- Still red: real-model router verification (§13, needs live inferd), model limitations (§9 list), UX items (§12), management intents + CLI keyword router (§13 future).

## 2026-07-31 — Docs aligned (e903768)

- AGENTS.md is the single origin for current + future state; this log records deltas only.
- Invalid state removed: `session`/`workspace` as the common-user path (power-user tooling only now, §12); silent chat fallback in the router (commit `3181178`, never to return).

## 2026-07-31 — FALLBACK removed from router (3181178)

- `detect_intent` → `Result`; failures print and exit(1), never silently route to chat.
- MockBackend can't produce intent JSON → router NLU mode-switching is red/unverified (§13).

## 2026-07-31 — Session/workspace commands added

- `roco session|workspace new/list/show/delete`, top-level `-p` flag, help text, 6 integration tests.
- Kept for power users; the common-user path is router-first (§12).

## 2026-07-31 — Flaky port tests fixed

- Save/restore env vars in gateway/inferd port default tests (parallel-run pollution).

## ✅ E2E green — story pipeline + resume + publish (real model)

- outline → wiki → chapters 1-3 (validated) → synopsis → publish. Revision retries converge in 2-3. `--resume` skips completed phases.
- Grammar + generation-loop fixes behind this (bnf_engine, gateway field-drop, schema enum scoping, actor double-gen) are documented in AGENTS.md §9/§10.
- Known red (model, not system): `val_instruction_following_matched`, in-string garbage under grammar mask, intermittent empty completions (~30-50%).

## ✅ CI green — 3 jobs (check / test / fmt)

- Removed `sccache-wrap` (not on runners), f64→f32 casts, fmt strict.

## 🟡 11 ignored tests = pending features

`cargo test --workspace -- --ignored` — streaming, interrupt, deadline, top-a, state blending, trace serde, error help text, mock grammar constraint, grammar mask validation.

## Migration (Phases 1–7 done)

Phase 3 `edf13bf` · Phase 4 `f1bed8e` · Phases 5–7 `3e75904` · out of scope: `crates/core`, `EvalCase` fixtures (DEPRECATED).
