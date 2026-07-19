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
