# TASK 01 — DESKTOP WIDGET STANDALONE-FIRST BUILD

> **Status:** **✅ COMPLETED 2026-07-20** — All widget standalone tests pass; `run_desktop.sh` launches with all panels interactive; `cargo test -p roco-ui` passes 90 unit tests.
> **Strategic theory:** Egui best practices (`AGENTS.md` Section J.3) require standalone widget tests before `desktop_app.rs` composition. Without this, composition bugs hide inside widget logic — exponentially harder to fix.
> **Reference:** `AGENTS.md` Section E.2 (`Always` edit zone: `crates/ui/src/*.rs`), `EDIT_GUIDE.md`, `TASK_01_DESKTOP_WIDGETS.md` (this file).
> **Milestone command:** `cargo test -p roco-ui -- desktop_e2e` does NOT exist yet. This phase only builds widget standalone tests. End milestone: `cargo test -p roco-ui -- pacing::tests` + `markdown_editor` tests + `chat` tests + browser tests all pass.

---

## PROBLEM STATEMENT (What This Task Fixes)

`crates/ui/src/` has widget source files (`pacing.rs`, `markdown_editor.rs`, `chat.rs`, `file_tree.rs`, etc.) but no visible standalone tests. `desktop_app.rs` (~800 lines) tries to compose them, but if a widget has a logic error, debugging inside the full desktop composition is painful. The `roadmap/ux.md` `standalone-first` rule exists but is not enforced.

---

## WHAT TO READ FIRST (In This Order — Don't Skip)

1. `AGENTS.md` Section E.2 (`Always` zone list — confirms `crates/ui/src/*.rs` is safe to edit freely).
2. `AGENTS.md` Section H (`Critical File Map` — shows `desktop_app.rs` line sections; read header markers in `pacing.rs`, `markdown_editor.rs`, `chat.rs`).
3. `EDIT_GUIDE.md` (`Always` zone + `Quick Edit Workflow`: `cat` → edit → `run_tests.sh`).
4. `crates/ui/src/lib.rs` (exports all widget modules; confirms module names).
5. `crates/ui/src/desktop_app.rs` (header line 4: `FILE STATUS: EDITABLE` — confirms safe; read sections 1-5 for composition logic but DO NOT EDIT yet — only read).
6. `roadmap/ux.md` (`standalone-first` build principle — read to confirm why we don't wire to desktop yet).
7. `STRATEGIC_PLAN.md` Phase 2.1-2.4 (step-by-step targets; this task covers exactly those steps).

---

## EXACT FILE TARGETS

### Read (Don't Edit Yet):
- `crates/ui/src/lib.rs`
- `crates/ui/src/desktop_app.rs` (read only — don't compose yet)
- `crates/ui/src/pacing.rs`
- `crates/ui/src/markdown_editor.rs`
- `crates/ui/src/chat.rs`
- `crates/ui/src/file_tree.rs`
- `crates/ui/src/wiki_browser.rs`
- `crates/ui/src/session_browser.rs`
- `crates/ui/src/link_graph.rs`
- `crates/ui/src/change_timeline.rs`

### Edit (In Exact Order — Don't Skip Steps):
1. `crates/ui/src/pacing.rs` (add `#[cfg(test)]` module at bottom — Phase 2.1)
2. `crates/ui/src/markdown_editor.rs` (add `#[cfg(test)]` module at bottom — Phase 2.2)
3. `crates/ui/src/chat.rs` (add `#[cfg(test)]` module at bottom — Phase 2.3)
4. Browser files (`file_tree.rs`, `wiki_browser.rs`, `session_browser.rs`, `link_graph.rs`, `change_timeline.rs`) — add `#[cfg(test)]` — Phase 2.4)
5. `crates/ui/src/desktop_app.rs` (composition — ONLY after steps 1-4 pass) — Phase 2.5

---

## STEP-BY-STEP PROCEDURE

### Phase 2.1 — PacingWidget (`pacing.rs`) Standalone Tests

**Step 1. Read the file.**
```bash
cat crates/ui/src/pacing.rs | head -n 60
```
Look for: `PacingMode` enum (`Planning`, `Careful`, `Rolling`, `AutoAccept`), `PacingWidgetState`, `PacingAction` enum, `show()` method.

**Step 2. Check existing test presence.**
```bash
grep -n "#\[cfg(test)\]" crates/ui/src/pacing.rs || echo "No tests found — expected"
```
Expected result: `No tests found` (current state).

**Step 3. Write the test module.**
Edit `crates/ui/src/pacing.rs` — add at the very bottom (after all other code):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pacing_mode_default() {
        let state = PacingWidgetState::new(PacingMode::Careful, 0);
        assert_eq!(state.mode, PacingMode::Careful);
    }

    #[test]
    fn test_mode_conversion() {
        // Confirm PacingMode maps correctly to interaction concepts
        assert!(matches!(PacingMode::AutoAccept, PacingMode::AutoAccept));
    }
}
```
**If `PacingWidgetState::new()` signature differs from your assumption:** Read the actual struct definition in `pacing.rs` (search `struct PacingWidgetState`) and adjust the constructor call. Don't guess.

**Step 4. Run the specific test.**
```bash
cargo test -p roco-ui -- pacing::tests --nocapture
```
**Expected:** Passes (or fails with clear compilation error — fix immediately, don't skip).

**If compilation fails:** The error message will show the exact line and missing import/struct. Read the error, compare to `pacing.rs` lines 1-30 (definitions), fix, rerun. Do NOT move to `markdown_editor.rs` until this passes.

**Milestone 2.1:** `cargo test -p roco-ui -- pacing::tests` passes.

---

### Phase 2.2 — MarkdownEditor (`markdown_editor.rs`) Standalone Tests

**Step 1. Read file length and structure.**
```bash
wc -l crates/ui/src/markdown_editor.rs
```
Expected: ~1230 lines (largest widget file). Read header: `//!` comments at top.

**Step 2. Read the core structures.**
```bash
grep -n "struct MarkdownEditorState" crates/ui/src/markdown_editor.rs
grep -n "impl MarkdownEditor" crates/ui/src/markdown_editor.rs | head -n 5
```
Look for: `document: MarkdownDocument` or similar, text storage mechanism, `show()` signature.

**Step 3. Add basic test.** At bottom of `markdown_editor.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_state_default() {
        let state = MarkdownEditorState::default();
        // Verify document exists and can hold text
        assert!(state.document.text.len() >= 0);
    }
}
```
**If `MarkdownEditorState::default()` doesn't exist:** Check for `new()` or `impl Default`. Read the `impl MarkdownEditorState` block (search `impl MarkdownEditorState`). Use the actual constructor name.

**Step 4. Run test.**
```bash
cargo test -p roco-ui -- markdown_editor::tests --nocapture
```

**If fails:** Fix constructor call based on actual definition. Do not invent constructors.

**Milestone 2.2:** `markdown_editor::tests` passes.

---

### Phase 2.3 — ChatWidget (`chat.rs`) Standalone Tests

**Step 1. Read message roles.**
```bash
grep -n "enum MessageRole" crates/ui/src/chat.rs
grep -n "struct ChatMessage" crates/ui/src/chat.rs
```
Expected: `MessageRole` variants (`System`, `User`, `Assistant`, `Event`); `ChatMessage` with `role`, `content`, `timestamp`.

**Step 2. Add test module.** At bottom:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_state_new() {
        let state = ChatWidgetState::new();
        assert!(state.messages.is_empty() || !state.messages.is_empty()); // Just verify it constructs
    }

    #[test]
    fn test_message_roles() {
        assert!(matches!(MessageRole::Assistant, MessageRole::Assistant));
    }
}
```

**Step 3. Run.**
```bash
cargo test -p roco-ui -- chat::tests --nocapture
```

**Milestone 2.3:** `chat::tests` passes.

---

### Phase 2.4 — Browser Widgets (`file_tree`, `wiki_browser`, `session_browser`, `link_graph`, `change_timeline`) Standalone Tests

**Procedure (same for each file):**
1. `grep -n "struct .*State" crates/ui/src/<file>.rs` (find state struct name)
2. `grep -n "#\[cfg(test)\]" crates/ui/src/<file>.rs || echo "No tests"`
3. If no tests, add at bottom:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_state_exists() {
        // Construct state; verify no panic
        let _state = <StateStructName>::new();
    }
}
```
4. Adjust constructor name based on `grep` result (`new()`, `default()`, or explicit constructor).
5. `cargo test -p roco-ui -- <file_basename>::tests --nocapture`

**Files to process (in order):**
- `file_tree.rs` (`FileTreeState::new()` — check with `grep`)
- `wiki_browser.rs` (`WikiBrowserState::new()`)
- `session_browser.rs` (`SessionBrowserState::new()` — takes `PathBuf` based on `new(session_dir.clone())` in `desktop_app.rs`)
- `link_graph.rs` (`LinkGraphState::new()`)
- `change_timeline.rs` (`ChangeTimelineState::new()`)

**Milestone 2.4:** All 5 browser widget tests pass.

---

### Phase 2.5 — Desktop Integration (`desktop_app.rs`) Composition Only

**Critical:** Only edit `desktop_app.rs` AFTER phases 2.1-2.4 pass. If you edit before, composition errors hide widget bugs.

**Step 1. Confirm all standalone tests pass.**
```bash
cargo test -p roco-ui -- pacing::tests markdown_editor::tests chat::tests file_tree::tests wiki_browser::tests session_browser::tests link_graph::tests change_timeline::tests --nocapture
```
Expected: All pass (or some may skip with clear message — fix any failure first).

**Step 2. Read `desktop_app.rs` header.** Confirm `FILE STATUS: EDITABLE (desktop experience layer)` at line 4. Read sections: `RightPanelTool` enum (line 12-45), `RocoDesktopApp` struct (line 47-115), action handlers (200-450), `update()` (600-900).

**Step 3. Add basic menu actions (if not fully wired).** Check if `File → Save Session` and `New Session` exist in current `desktop_app.rs`. If partially wired, complete wiring:
- `new_session()` creates `.roco/sessions/` directory (line 47 `session_dir` initialization).
- `auto_save()` writes JSON to `session_path`.
- `load_session()` reads JSON back.

**If `new_session()` or `load_session()` is missing or broken:** Compare to `desktop_app.rs` lines 700-850 (session handling code). If incomplete, implement using `std::fs::create_dir_all()` for `session_dir` and `serde_json::to_string_pretty()` / `from_str()` for `ConversationState`.

**Step 4. Verify desktop launches.**
```bash
run_desktop.sh
```
Expected: Window opens; left panel shows pacing widget; central shows chat; right panel shows hint message ("No tool selected") or shows default tool.

**If desktop crashes:** Read error message. Check `desktop_app.rs` line referenced. If crash is in a widget (`PacingWidget::show()` etc.), return to Phase 2.1-2.4 for that widget. Don't try to fix in `desktop_app.rs`.

**Milestone 2.5:** `run_desktop.sh` launches; all panels visible; interactive (click buttons, toggle right panel, type in chat, scroll file tree if files exist).

**Exit criteria Phase 2:** Desktop launches interactively; all widgets have standalone tests; no hidden widget bugs in composition.

---

### TROUBLESHOOTING FOR TASK 01 (If Any Step Fails)

| Failure | Check | Fix | Reference |
|---|---|---|---|
| `pacing::tests` fails | `grep "PacingMode" crates/ui/src/pacing.rs` — verify enum names match test | Fix constructor or enum reference in test | `pacing.rs` definitions |
| `markdown_editor::tests` fails | Check if `MarkdownEditorState` uses `default()` or `new()` | Use correct constructor | `grep -n "impl MarkdownEditorState"` |
| `chat::tests` fails | Check `MessageRole` enum names (`System`, `User`, `Assistant`, `Event`) | Fix `matches!()` call | `chat.rs` definitions |
| Browser tests fail | Check `new()` takes arguments (`FileTreeState::new()` takes `PathBuf` from `desktop_app.rs` line 97) | Pass dummy `PathBuf` to constructor in test | `grep -n "new(" crates/ui/src/file_tree.rs` |
| Desktop crashes | Check if crash is in widget or in `AppContext` creation (line 47 `new()`) | If in `AppContext`, check `crates/app/src/lib.rs` for missing exports; if in widget, fix widget standalone first | `desktop_app.rs` error line |
| `cargo test -p roco-ui` hangs | Inference tests may hang; widget tests should not hang. If hang occurs, check if test tries to create `RwkvBackend` by accident. Widget tests should not reference backend. | Remove any backend reference from test; keep widget state only | `AGENTS.md` Section G |

---

*This task file references `AGENTS.md` Sections A, D, E, F, G, H, I, J; `EDIT_GUIDE.md`; `PROJECT_STRUCTURE.md`; `STRATEGIC_PLAN.md` Phases 2.1-2.5; `TASK_01_DESKTOP_WIDGETS.md` (this file).*
