# TASK 07 — WEB FREEZE + DEPRECATION DOCUMENTATION

> **Reference:** `AGENTS.md` Section E.2 (`Always`: `crates/ui/src/*.rs`; `Edit only for bug fixes`: `apps/*` — `STRATEGIC_PLAN.md` Phase 4.4); `EDIT_GUIDE.md`; `PROJECT_STRUCTURE.md`.
> **Status:** **✅ COMPLETED 2026-07-20** — README updated (desktop → primary surface, web → legacy/frozen). PROJECT_STRUCTURE updated. `apps/chat/`, `apps/studio/`, `apps/editor/` frozen for new features.
> **Why:** `STRATEGIC_PLAN.md` Phase 4.4: desktop is primary tested surface; untested web apps create split-brain maintenance.

---

## WHAT TO DO (No Code Changes To Web Apps Unless Bug Fix)

### Step 1 — Confirm Web App Status (Read Only)
```bash
ls apps/chat/app/page.tsx apps/studio/app/page.tsx apps/editor/index.html
```
Confirm all exist and are unchanged (no new feature files added recently unless documented).

---

### Step 2 — Document Freeze In `README.md`

In `README.md`, find `## Three Surfaces` or `### What This Is` section. Confirm it states desktop is primary and web apps are secondary/deprecated. If missing or unclear:

Edit `README.md` (line near section header):
Add sentence: `Note: Web apps (apps/chat/, apps/studio/, apps/editor/) are maintained for plugin compatibility and external integrations but are not the primary user surface. New user-facing features are developed in crates/ui/ (desktop GUI) first.`

**Only edit `README.md`** — do not edit `apps/*/` source files unless fixing a documented bug.

---

### Step 3 — Document Freeze In `PROJECT_STRUCTURE.md`

Confirm `PROJECT_STRUCTURE.md` table shows `crates/ui/` as planned primary; `apps/` as deprecated/migrating. If missing, edit `PROJECT_STRUCTURE.md` (line near `apps/` row):
Add note: `Deprecated for new feature development. Bug fixes only. Migration target: crates/ui/ (desktop widgets).`

---

### Step 4 — Confirm No New Features Added To Web Apps

```bash
git log --oneline -- apps/chat/app/page.tsx apps/studio/app/page.tsx apps/editor/src/main.ts | head -n 10
```
Check recent commits. Confirm no new feature commits since `AGENTS.md` v2.0 (2026-07-20) unless documented as bug fixes.

---

### Step 5 — Freeze Policy Enforcement (Documentation Only — No Source Changes)

Create or update `TASK_07_WEB_FREEZE.md` (this file) with:
- Freeze date (`2026-07-20` or when `AGENTS.md` v2.0 created).
- Exception list: bug fixes allowed; plugin API updates allowed; new user-facing features NOT allowed.
- Migration path reference (`TASK_04_EDITOR_MIGRATION.md`, `TASK_05_CHAT_MIGRATION.md`, `TASK_06_STUDIO_INTEGRATION.md`).

---

## TROUBLESHOOTING (If Freeze Violated)

| Issue | Check | Fix |
|---|---|---|
| Web app edited with new feature | `git diff apps/chat/app/page.tsx` shows feature addition | Revert feature to `crates/ui/` desktop widget. Document in `TASK_04-06` files. Update freeze note if feature needed in web for plugin compatibility (ask user first — `AGENTS.md` Section E.3). |
| Plugin (`vscode/`) requires server endpoint that desktop doesn't provide | Check `API.md` and `TASK_08_PLUGIN_VERIFY.md` | Server endpoint (`roco-server`) remains available for plugins. Desktop uses direct backend (`AppContext`); plugins can connect to either. Confirm `PLUGINS.md` reflects this. Don't change desktop to force server dependency. |
| Web app bug found but fix requires desktop feature | Confirm bug is real web-only issue; check if same bug exists in desktop widget | Fix web bug minimally. If desktop has same issue, fix desktop widget (`TASK_01_DESKTOP_WIDGETS.md` or `TASK_02_DESKTOP_INTERACTION.md`) instead of duplicating fix in web. |

---

*This task references `AGENTS.md` Sections D, E.2, G; `EDIT_GUIDE.md`; `TASK_04_EDITOR_MIGRATION.md`; `TASK_05_CHAT_MIGRATION.md`; `TASK_06_STUDIO_INTEGRATION.md`; `TASK_08_PLUGIN_VERIFY.md`; `STRATEGIC_PLAN.md` Phase 4.4.*
