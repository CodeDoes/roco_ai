# PROGRESS.md — Phase 8 Verification Log

Lean delta log — one entry per change, newest first. Canonical state lives in AGENTS.md (§9 loop, §10 known issues, §12 UX, §13 router NLU). Old history is in git.

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
