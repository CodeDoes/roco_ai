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

## 2026-07-19 (cont.)
- Added `crates/ui` with egui dependency. Implemented **PacingWidget**
  (standalone, tested) mapping UX pacing modes (Planning/Careful/Rolling/Auto-Accept)
  to tested `roco_agent::interaction::InteractionMode` (NoControl/FullControl/
  ModerateControl/GoHam). 11 unit tests pass covering mode conversion, state
  sync, pause logic, progress display, and action emission. Engine unchanged
  (path dep only). Meets Definition of Done for widget: surface + control +
  tested.

## 2026-07-19 (cont.)
- Implemented **MarkdownEditor** widget — the PRIMARY SURFACE (prose is the product).
  Features: per-range MS-Word-style comments, inline AI generate/replace actions
  for selections, diff view for AI suggestions, accept-section/accept-selection
  for suggestions, built on egui::TextEdit + Painter overlays (Lockbook-style),
  undo/redo via document versioning, keyboard shortcuts (Ctrl+Z/Y/S).
  13 unit tests pass covering document ops, suggestions, comments, undo/redo,
  and action application. Meets Definition of Done for widget: surface + control +
  tested + reversible (undo stack).

## 2026-07-19 (cont.)
- Implemented **ChatWidget** — the conversation surface. Features: 7 message
  part types (system/user/assistant/think/tool_call/tool_result/event) with
  role-colored badges and timestamps; streaming indicator; text input with
  Enter-to-send; capabilities panel (6 toggles: Generate/Research/Edit/Critique/
  Outline/Brainstorm); context info panel (document, section, selection, tokens);
  attachments bar; compact mode for side panels. 15 unit tests passing.
  Meets Definition of Done: surface + control + tested + reversible (undo/redo
  actions wired).
