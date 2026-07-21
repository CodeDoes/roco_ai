# TASK 06 — STUDIO INTEGRATION (Desktop Right-Panel Composition)

> **Reference:** `AGENTS.md` Sections A, D, E.2, G; `EDIT_GUIDE.md`; `STRATEGIC_PLAN.md` Phase 4.3; `TASK_02_DESKTOP_INTERACTION.md` Phase 2.5 completed (desktop launches interactively); `TASK_05_CHAT_MIGRATION.md` completed (chat mechanism verified).
> **Status:** **✅ COMPLETED 2026-07-20** — Already complete via Phase 2.5 (composition) + Phase 3 (interaction flow). Left panel: pacing/tools. Center: chat. Right panel: editor, file tree, wiki, link graph, sessions, timeline, quality (switchable via `RightPanelTool`).
> **Rule:** Only compose existing tested widgets. Don't invent new widget logic. `desktop_app.rs` is `Always` edit zone.

---

## PROBLEM (Why Composition Matters)

Desktop launches (`TASK_02_DESKTOP_INTERACTION.md` Milestone 2.5) but right panel shows only single tool (`RightPanelTool::Editor` or `FileTree` individually). The writer needs a unified workspace: edit text (`Editor`), browse files (`FileTree`), see plot links (`LinkGraph`), manage sessions (`Sessions`), view timeline (`Timeline`), reference wiki (`Wiki`), and chat (`Chat` — center panel, not right). `apps/studio/` combines these; desktop should match this experience using tested `crates/ui/` widgets.

---

## PREREQUISITE (Don't Start If Missing)

```bash
# Confirm desktop launches with all panels visible
run_desktop.sh
# Confirm right panel toggles between Editor, FileTree, Wiki, LinkGraph, Sessions, Timeline
# Confirm chat responds (even dummy)
```
**If any widget missing or broken:** Return to `TASK_01_DESKTOP_WIDGETS.md` or `TASK_02_DESKTOP_INTERACTION.md` — don't compose broken widgets.

---

## WHAT TO READ

- `AGENTS.md` Section H (`Critical File Map`: `desktop_app.rs` sections 4-5 show panel wiring).
- `crates/ui/src/desktop_app.rs` lines 450-600 (`show_right_panel()` — how each `RightPanelTool` renders its widget).
- `crates/ui/src/lib.rs` (confirms all widget modules exported).
- `STRATEGIC_PLAN.md` Phase 4.3 (`Studio Integration` — target: unified interface).
- `TASK_06_STUDIO_INTEGRATION.md` (this file — reference for troubleshooting).

---

## MIGRATION / COMPOSITION TARGETS

| Studio Component (`apps/studio/`) | Desktop Widget (`crates/ui/src/`) | Composition Requirement |
|---|---|---|
| File browser (`file-browser.tsx`) | `FileTree` (`file_tree.rs`) | Already wired via `RightPanelTool::FileTree` in `show_right_panel()` |
| Editor (`editor-panel.tsx`) | `MarkdownEditor` (`markdown_editor.rs`) | Wired via `RightPanelTool::Editor` |
| Chat (`chat-panel.tsx`) | `ChatWidget` (`chat.rs`) | Central panel — already composed |
| Wiki (`wiki-browser.tsx` — implied) | `WikiBrowser` (`wiki_browser.rs`) | Wired via `RightPanelTool::Wiki` |
| Agents manager (`agents-manager.tsx`) | No direct widget; use `SessionBrowser` (`session_browser.rs`) + agent binding through `AppContext::session_agent()` | Confirm `AppContext` has agent binding (`session_agent` capability — `AGENTS.md` Section D). If missing, document gap (don't invent `AppContext` method). |
| Link graph / plot visualization | `LinkGraph` (`link_graph.rs`) | Wired via `RightPanelTool::LinkGraph` |
| Session timeline / version control | `ChangeTimeline` (`change_timeline.rs`) | Wired via `RightPanelTool::Timeline` |
| Pacing control | `PacingWidget` (`pacing.rs`) | Left panel — already composed |

---

## STEP-BY-STEP COMPOSITION PROCEDURE

### Step 1 — Confirm All Right-Panel Tools Exist And Render

```bash
grep -n "RightPanelTool::" crates/ui/src/desktop_app.rs | head -n 20
```
Look for: `RightPanelTool::Editor`, `FileTree`, `Wiki`, `LinkGraph`, `Sessions`, `Timeline` all referenced in `show_right_panel()`.

**If any tool missing from `show_right_panel()`:** Check `RightPanelTool` enum definition (line ~12-45). Confirm all variants declared. Confirm `show_right_panel()` handles each variant (`Some(RightPanelTool::X)` branch exists). Don't invent new tool variants — use existing ones.

---

### Step 2 — Confirm Widget States Initialize Without Errors

```bash
sed -n '47,115p' crates/ui/src/desktop_app.rs
```
Look for: Each widget state initialized (`pacing_state`, `chat_state`, `editor_state`, `file_tree_state`, `wiki_state`, `link_graph_state`, `session_browser_state`, `timeline_state`). Confirm no `panic!()` or unhandled initialization errors.

**If initialization fails for a new widget state:** Read widget file (`wiki_browser.rs`, `link_graph.rs`, etc.) for `new()` constructor. Confirm constructor arguments match what's passed in `desktop_app.rs` `new()` (e.g., `WikiBrowserState::new()` takes no args; `LinkGraphState::new()` takes no args; `SessionBrowserState::new(session_dir.clone())` takes `PathBuf`).

---

### Step 3 — Confirm Agent Manager Integration (If `AppContext` Supports It)

```bash
grep -n "session_agent\|AgentConfig\|agent" crates/app/src/lib.rs | head -n 10
```
Look for: `session_agent` capability. Confirm `AppContext::session_agent()` exists or `SessionAgent` is exported.

**If agent binding exists:** Wire through `AppContext` in desktop `new()` or through session management (`New Session` creates default agent, or user selects agent persona). Document in `desktop_app.rs` — don't invent new agent selection UI unless user requests.

**If agent binding missing:** Document gap. Don't invent `AppContext` methods. `AGENTS.md` Section E.3 (`Ask First`) requires confirmation for new surface primitives.

---

### Step 4 — Confirm Studio Panel Composition Works Manually

Run desktop (`run_desktop.sh`). Manually verify:
1. Left panel (`PacingWidget` + session info + new/save buttons + tool quick-launch) — interactive.
2. Central panel (`ChatWidget` — type and send; message history visible; streaming or dummy response appears).
3. Right panel (`RightPanelTool` toggles):
   - `Editor`: opens file from workspace; shows text; editable.
   - `FileTree`: shows `.roco/workspaces/` contents; file selection updates editor (`TASK_04_EDITOR_MIGRATION.md` Milestone 3.1).
   - `Wiki`: renders wiki content (if `.roco/workspaces/` has `02-WIKI.md` or user creates one).
   - `LinkGraph`: shows nodes (`protagonist`, `antagonist`, etc.) and edges (from `desktop_app.rs` initialization). Confirm interactive (click node updates status).
   - `Sessions`: lists `.roco/sessions/*.json`; load/delete works (`TASK_02_DESKTOP_INTERACTION.md` Phase 3.1 milestone).
   - `Timeline`: shows checkpoint/snapshot entries (add `Create Snapshot` through menu or button if needed — confirm `handle_timeline_action()` responds).

**Milestone 4:** All 7 `RightPanelTool` variants accessible; left and center panels interactive; no crashes when toggling between right-panel tools rapidly.

---

### TROUBLESHOOTING FOR TASK 06

| Composition Issue | Check | Fix / Decision |
|---|---|---|
| `LinkGraph` or `Wiki` crashes when shown | Widget `new()` constructor takes unexpected args; check `desktop_app.rs` initialization vs widget `new()` signature | Adjust initialization parameters to match. Don't modify widget `new()` unless widget file header confirms safe (`Always` zone). |
| Agent manager not integrated | `AppContext::session_agent()` missing or `AGENTS.md` Section E.3 applies | Confirm with `cat crates/app/src/lib.rs` capabilities list. If missing, ask user (don't invent `AppContext` method). |
| Timeline doesn't show entries | `ChangeTimelineState::new()` creates empty timeline; `timeline_state.add_entry()` not called on session start | Confirm `new_session()` calls `timeline_state.add_entry()`. If missing in `desktop_app.rs`, add using pattern from `new_session()` method (line ~700-720). |
| Studio interface feels fragmented | User expects unified experience like `apps/studio/` (chat + editor + file browser + agents) | Confirm desktop provides all through left/center/right panels. If user wants more unified view (e.g., split editor in central panel instead of right), ask before restructuring `desktop_app.rs` layout. `Always` edit zone allows layout changes, but user experience changes need user input. |
| Right panel shows "No tool selected" always | `right_panel_tool` never set to `Some()`; `show_right_panel()` handles `None` with hint message | Confirm menu `View` toggles `Some()` value. Confirm `new()` doesn't set `None` permanently. If `None` is default, user must click menu item — this is design choice, not bug. Confirm with user if automatic right-panel default needed. |

---

*This task references `AGENTS.md` Sections A, D, E.2, H; `EDIT_GUIDE.md`; `TASK_01_DESKTOP_WIDGETS.md` (prerequisite); `TASK_02_DESKTOP_INTERACTION.md` (prerequisite Phase 2.5); `TASK_05_CHAT_MIGRATION.md` (chat mechanism); `STRATEGIC_PLAN.md` Phase 4.3.*
