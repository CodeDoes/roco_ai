# Blocked / Open Questions

> A parking lot, not a todo list. Things waiting on a human decision.

- **gpui vs keep-webapp:** DECIDED 2026-07-19 — **gpui**. The UI moves into
  the same tested Rust tree as the engine. Retire the untested `apps/`
  webapps (chat/studio/editor/plugins). The desktop app is the primary
  surface; `crates/tui` (ratatui) is superseded by the gpui app.
- **Where does the CLI (`crates/cli`) fit** if the desktop app becomes the
  primary surface? Keep as a headless/scriptable counterpart, or retire?
- **Export formats** (markdown / epub / pdf) — deferred; depends on whether
  the human wants publish-from-desktop.
