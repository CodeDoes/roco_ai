# Progress Log

> Append-only. After a meaningful change, add a dated line: what changed, where,
> and whether it meets the Definition of Done (surface / control / tested /
> reversible). Keep it one or two lines.

## 2026-07-19
- Removed `goals/` and `PROGRESS.md`. They were a scratch roadmap that steered
  the agent toward engine completeness (✅ in crates) instead of human-centric
  experience. Replaced by `roadmap/`.
- Rewrote `AGENTS.md` to center UX, the Definition of Done, and agent behavior
  guidance. Engine declared frozen; new work is frontend-only.
- Decision: migrating off the untested webapps (`apps/`) toward a gpui desktop
  app so the UI lives in the same tested Rust tree as the engine.
- gpui REJECTED — not for common use (giant Zed git fetch, unstable API,
  stalled build). Switched to **egui** (crates.io, immediate mode). Reverted
  the `crates/app` gpui spike and the gpui workspace dep; `Cargo.lock`
  regenerated clean (0 gpui entries). Updated `roadmap/ux.md` with the full
  widget spec + standalone-first build principle.
- Widget spec locked: MD editor with **per-range** MS-Word-style comments
  (required for prose diffs), inline AI generate/replace, range-level diff +
  accept-section/accept-selection; chat with pacing control; file tree, wiki
  browser, link graph, session browser, change timeline. Build rule: each
  widget tested standalone before composition. tmux 3.6 now installed.
