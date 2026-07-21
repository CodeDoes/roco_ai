# Roadmap — RoCo AI

> Living plan. Read this first for any feature work.

## Current Milestone: v1 (Experience Complete)

**Primary doc:** [`v1.md`](v1.md) — checklist, status, release criteria.

## Supporting Docs

| File | Purpose |
|------|---------|
| [`ux.md`](ux.md) | Human experience spec (widgets, pacing modes, feedback loop) |
| [`progress.md`](progress.md) | Append-only change log |
| [`blocked.md`](blocked.md) | Parking lot for open questions |

---

## Quick Orientation

| I want to... | Go here |
|--------------|---------|
| See what's done / in progress / blocked for v1 | [`v1.md`](v1.md) |
| Understand the UX design (widgets, pacing, feedback) | [`ux.md`](ux.md) |
| See chronological history of changes | [`progress.md`](progress.md) |
| Find open questions needing human decision | [`blocked.md`](blocked.md) |

---

## Milestone History

- **2026-07-19**: Engine declared frozen; pivot to desktop (egui); Phase 2 standalone widget build started
- **2026-07-19**: PacingWidget, MarkdownEditor, ChatWidget completed + tested
- **2026-07-19**: `roco interact` CLI, `RocoDesktopApp` GUI, daemon lifecycle completed
- **2026-07-20**: All 6 browser panels wired; 81 unit + 9 user-story tests pass; docs rewritten
- **2026-07-21**: **Dead code removed** (tui, web apps, probe examples); 2 test failures fixed; all 400+ tests pass; docs updated to v4.0