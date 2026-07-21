# Blocked / Open Questions

> A parking lot, not a todo list. Things waiting on a human decision.

- **GUI framework:** DECIDED 2026-07-19 — **egui** (rejected gpui). gpui is
  not for common use (only via ~1GB Zed git fetch, unstable API, stalled
  the build). egui is on crates.io, mature, immediate-mode — fits streaming
  text + controls. `egui_markdown` for rendered markdown; the editable
  markdown editor is custom-built (egui has no native rich editor). See
  `roadmap/ux.md`.
- **tmux:** now installed (3.6) — available for driving `pi` in a detached
  session if needed.
- **Where does the CLI (`crates/cli`) fit** if the desktop app becomes the
  primary surface? Keep as a headless/scriptable counterpart, or retire?
- **Export formats** (markdown / epub / pdf) — deferred; depends on whether
  the human wants publish-from-desktop.