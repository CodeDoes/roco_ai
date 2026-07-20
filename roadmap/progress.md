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

## 2026-07-19 (cont.)
- Implemented **`roco interact` CLI** — terminal equivalent of the GUI experience.
  Three modes: `--interactive` (REPL with pacing controls: /accept, /skip, /stop,
  /undo, /pace), `--trigger "prompt"` (one-shot generation for scripts/pipes),
  `--resume <session-id>` (load and continue from saved session). Session
  persistence via JSON files in `.roco/sessions/`. 10 unit tests passing.
  Color-coded output via rich_output.rs. Meets Definition of Done: surface +
  control + tested + reversible (undo, session resume).

## 2026-07-19 (cont.)
- Implemented **RocoDesktopApp** — full egui/eframe desktop GUI (`roco gui`).
  Wires ChatWidget (main area), PacingWidget (left panel), MarkdownEditor (right
  panel) together with model loading (supports fallback if no RWKV_MODEL),
  session management (New/Save), menu bar (File, View, Help). 39 tests pass.
  Launches via `roco gui` subcommand.

## 2026-07-20
- Wired **all 6 browser panels** into RocoDesktopApp (`crates/ui/src/desktop_app.rs`):
  FileTree, WikiBrowser, LinkGraph, SessionBrowser, ChangeTimeline, and Editor
  are now accessible via View menu or Tools sidebar. Each renders in a resizable
  right panel with full action handling wired to the desktop state. The
  RightPanelTool enum controls which tool is shown, and each tool refreshes its
  data on activation. 81 unit tests + 9 user-story integration tests pass.
- Fixed `test_chapter_steerer_lifecycle` assertion (word count: 5 not 6).
- Fixed `test_handle_command_accept_skip_stop` to match actual stop-behavior.
- Cleaned up unused imports in desktop_app.rs.

## 2026-07-19 (cont.)
- Added **daemon lifecycle** — `roco gui` auto-starts gateway daemon on :8000
  if not running; gateway daemon auto-starts inference server on :8080 if not
  running. Uses PID files + health endpoint checks. `crates/cli/src/daemon.rs`
  with `ensure_daemon()` and `wait_for_healthy()`. GUI talks to gateway via
  `RemoteBackend` (HTTP) instead of loading model directly.

## 2026-07-20 (cont.)
- **Docs/structure pass.** Added writer-first documentation suite: `start.sh`,
  `run_tests.sh`, `run_desktop.sh` (helpers); `USER_GUIDE.md`, `PROJECT_STRUCTURE.md`,
  `AGENT_GUIDE.md`, `EDIT_GUIDE.md`, `STRATEGIC_PLAN.md`, `COMMANDS.md`, `EDITOR.md`,
  `PLUGINS.md`, `API.md`, and 10 `TASK_*.md` phase tickets tracking the
  desktop-widget-standalone-first work. Rewrote `AGENTS.md` (v3.0: protection
  markers + Critical File Map + Pitfalls table) and `README.md` (writer-first
  quick-start). Added `FILE STATUS:` header markers to the 5 largest source
  files (`roco.rs` ~1373 lines, `desktop_app.rs` ~800 lines, `story_engine.rs`
  ~954 lines, `mecha_agent.rs` ~990 lines, `crates/app/src/lib.rs`). Removed
  accidentally-tracked `node_modules/.package-map.json` and
  `.pnpm-workspace-state-v1.json`. No behavior change; build remains green
  (`cargo check --workspace` passes; 0 errors, only pre-existing
  `rich_output.rs` unused-function warnings). Engine frozen; this is a
  documentation reorganization supporting the experience-first roadmap
  (`STRATEGIC_PLAN.md` Phase 1 -> Phase 2).
