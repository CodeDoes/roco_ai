# TASK 04 — EDITOR MIGRATION (Web Editor → Desktop `MarkdownEditor`)

> **Reference:** `AGENTS.md` Section E.2 (`Always` zone: `crates/ui/src/*.rs`); `EDIT_GUIDE.md`; `STRATEGIC_PLAN.md` Phase 4.1; `TASK_02_DESKTOP_INTERACTION.md` Phase 3.5 completed.
> **Status:** Migration target (`apps/editor/` features → `crates/ui/src/markdown_editor.rs`).
> **Rule:** No new features in `apps/editor/`. Only migration of existing capabilities to desktop widget.

---

## PROBLEM (Migration Scope)

`apps/editor/` (`index.html`, `src/main.ts`, `src/api.ts`) provides rich markdown editing, file tree navigation, auto-save, and server connection. Desktop `MarkdownEditor` (`markdown_editor.rs`, ~1230 lines) exists but may not have all these features. This task moves verified capabilities from web to desktop, not inventing new ones.

---

## PREREQUISITE CHECKLIST

```bash
# Desktop E2E test exists and passes (or at least workspace creation confirmed)
test -f crates/ui/src/tests/desktop_e2e.rs || echo "E2E file missing — complete TASK_03 first"
```
**If E2E file missing:** Return to `TASK_03_DESKTOP_END_TO_END.md` Step 2. Do not proceed with migration.

---

## WHAT TO READ (Specific To Migration)

- `AGENTS.md` Section D (`Architecture` diagram — confirms `AppContext` → workspace → file persistence flow).
- `AGENTS.md` Section E.2 (`crates/ui/src/markdown_editor.rs` editable; `crates/app/src/lib.rs` caution zone).
- `crates/ui/src/markdown_editor.rs` (full file — read header line 4 for status; read `MarkdownEditorState` definition; check `show()` method for current capabilities).
- `crates/ui/src/file_tree.rs` (check if file browser integration exists — needed for editor to navigate workspace files).
- `apps/editor/src/main.ts` (read full — understand web editor features: markdown editing, file opening, save mechanism).
- `apps/editor/src/api.ts` (understand server connection — desktop uses direct `RwkvBackend`, not server HTTP; document any feature that depends on server-only functionality).
- `crates/workspace/src/workspace.rs` (understand workspace file operations — needed for auto-save).

---

## EXACT MIGRATION TARGETS (From Web To Desktop)

| Web Feature (`apps/editor/`) | Desktop Target (`crates/ui/src/`) | Migration Type | Verification |
|---|---|---|---|
| Markdown text editing (`main.ts`) | `markdown_editor.rs` `MarkdownEditorState` + `show()` | Confirm existing or extend | Editor renders text; basic input handled (`TASK_01_DESKTOP_WIDGETS.md` 2.2 milestone) |
| File opening (`main.ts` — opens file from workspace) | `file_tree.rs` `FileTree` selection → `MarkdownEditor` text load | Wire existing: `FileTree` action already loads file into editor? Check `desktop_app.rs` `handle_file_tree_action()` (line ~250) for `OpenFile` action. Confirm it sets `editor_state.document.text`. | `TASK_02_DESKTOP_INTERACTION.md` Phase 3.1 milestone |
| Auto-save (`main.ts` — saves to workspace) | `AppContext::workspace_transform()` or `Workspace::resolve()` + file write | Confirm `AppContext` or desktop `auto_save()` writes to workspace. Check `desktop_app.rs` `auto_save()` method (line ~850). Confirm it writes `ConversationState` to session path. For editor, confirm workspace file save mechanism exists in `markdown_editor.rs` or `workspace.rs`. | Manual: edit text in desktop editor, check workspace file updates |
| Syntax highlighting (`main.ts`) | `egui_markdown` integration (`markdown_editor.rs`) | Check if `markdown_editor.rs` uses `egui_markdown`. Search `grep -n "egui_markdown" crates/ui/src/markdown_editor.rs`. If present, feature exists. If not, add basic markdown rendering. | Visual confirmation: markdown text shows formatting |
| Server connection (`api.ts`) | Direct `RwkvBackend` (`AppContext` → backend) or `roco-server` (optional) | Document decision: desktop uses direct backend for core; server remains for plugins/web. Check `desktop_app.rs` `new()` backend parameter (`Option<Arc<dyn ModelBackend>>`). Confirm `AppContext` connects to this backend. | `run_desktop.sh` launches with backend loaded (if `RWKV_MODEL` set) or `None` (dummy mode) |

---

## STEP-BY-STEP MIGRATION PROCEDURE

### Step 1 — Confirm Web Features (Read Only)

```bash
cat apps/editor/src/main.ts
cat apps/editor/src/api.ts
```
Look for: file opening mechanism (`readFile` or `fetch`), save mechanism (`writeFile` or `post` to server), markdown rendering library (`marked` or `markdown-it`), syntax highlighting (`highlight.js` or similar).

**If any web feature uses server-only endpoint that desktop doesn't replicate:** Document the gap. Don't invent desktop equivalent — ask user if feature should migrate (via server) or be dropped.

---

### Step 2 — Confirm Desktop Editor Capabilities

```bash
grep -n "show\|document\|text\|save\|load" crates/ui/src/markdown_editor.rs | head -n 20
```
Look for: `show()` method renders `MarkdownEditorState`; state has `document` with `text` field; save/load mechanism exists or can be added through workspace.

**If `MarkdownEditorState` has no `document.text` or no rendering mechanism:** Read full file. Confirm structure. Don't invent — ask user if editor needs extension or if current state is sufficient for basic text editing (milestone: editing `01-OUTLINE.md` text works).

---

### Step 3 — Wire File Tree → Editor (If Not Already Wired)

In `desktop_app.rs`, check `FileTreeAction::OpenFile` handler (line ~250-300):
```bash
sed -n '250,320p' crates/ui/src/desktop_app.rs
```
Look for: `self.editor_state.document.text = content;` and `self.right_panel_tool = Some(RightPanelTool::Editor);`

**If missing or broken:** Add or fix wiring:
```rust
FileTreeAction::OpenFile(path) => {
    self.status_message = format!("Opened: {}", path.display());
    if let Ok(content) = std::fs::read_to_string(&path) {
        self.editor_state.document.text = content;
        self.right_panel_tool = Some(RightPanelTool::Editor);
    }
}
```
Only edit `desktop_app.rs` — don't modify `markdown_editor.rs` for this step (wiring, not widget logic).

**Milestone 3.1:** User clicks file in `FileTree`; editor shows content; `right_panel_tool` switches to `Editor`.

---

### Step 4 — Confirm Auto-Save (Workspace Persistence)

In `desktop_app.rs`, check `auto_save()` method:
```bash
sed -n '840,870p' crates/ui/src/desktop_app.rs
```
Look for: `std::fs::write()` to `session_path`. Confirm `session_path` points to `.roco/sessions/` or workspace directory.

**If auto-save writes session JSON (`ConversationState`) but not workspace files:** Confirm editor changes don't persist to workspace automatically. Add workspace file save to `MarkdownEditor` or `AppContext` if needed. But document gap: `MarkdownEditor` may need explicit save action (button/menu) for workspace files, unlike session auto-save.

**Don't invent `MarkdownEditor` save mechanism** — check if `AppContext::workspace_transform()` or `Workspace` has file write method (`workspace/src/workspace.rs`). If `Workspace` only manages directories and `AppContext` doesn't expose file writes for editors, document the gap rather than inventing new `AppContext` methods (caution zone per `EDIT_GUIDE.md`).

**Milestone 3.2:** Editor shows file content; workspace directory exists; session save works.

---

### Step 5 — Add Markdown Rendering (If `egui_markdown` Not Present)

Check:
```bash
grep -n "egui_markdown\|markdown" crates/ui/src/markdown_editor.rs | head -n 10
```
**If `egui_markdown` not referenced:** Basic markdown rendering may not be implemented. Confirm `MarkdownEditor` renders plain text at minimum. If rendering is needed but missing, ask user for design decision (add `egui_markdown` dependency? Use `egui` `RichText` manually?). Don't invent rendering logic without user input.

**Milestone 3.3:** Editor displays text; basic editing works; file navigation works; workspace persistence confirmed (either automatic or manual — document which).

---

### TROUBLESHOOTING FOR TASK 04 (Migration Issues)

| Migration Step Fails | Check | Fix / Decision |
|---|---|---|
| File tree → editor load broken | `FileTreeAction::OpenFile` handler missing `document.text` assignment | Fix wiring in `desktop_app.rs` (Step 3) |
| Editor text doesn't save | `MarkdownEditor` state has no save mechanism; `AppContext` has no workspace file write | Check `workspace/src/workspace.rs` for file operations. If missing, ask user — don't invent `AppContext` method. Document gap. |
| `egui_markdown` missing | `grep` shows no reference | Confirm basic text editing works. If rich rendering needed, ask user before adding dependency. Don't invent. |
| Web feature requires server endpoint | `apps/editor/src/api.ts` uses `/generate` or `/story/outline` server-only | Desktop uses `AppContext` direct backend. Document server feature as plugin/external only. Don't migrate server-only features to desktop unless user confirms. |

---

*This task references `AGENTS.md` Sections C, D, E.2, G; `EDIT_GUIDE.md`; `TASK_01_DESKTOP_WIDGETS.md` (prerequisite); `TASK_02_DESKTOP_INTERACTION.md` (prerequisite); `STRATEGIC_PLAN.md` Phase 4.1.*
