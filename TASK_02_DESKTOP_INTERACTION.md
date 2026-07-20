# TASK 02 — DESKTOP FULL INTERACTION FLOW + QUALITY INTEGRATION

> **Reference:** `AGENTS.md` Sections A, C, D, E, G, H; `EDIT_GUIDE.md`; `STRATEGIC_PLAN.md` Phase 3; `TASK_01_DESKTOP_WIDGETS.md` (prerequisite: Phase 2.5 complete).
> **Target:** 3 weeks (Phase 3.1-3.5 combined).
> **Prerequisite milestone:** `run_desktop.sh` launches; all widget standalone tests pass (`TASK_01_DESKTOP_WIDGETS.md` Milestone 2.5).

---

## PROBLEM (What This Fixes)

Desktop (`desktop_app.rs`) launches and shows widgets, but cannot complete a writer's story journey: no `AppContext` wired to generation, no interactive pacing mapped to story engine, no quality feedback visible, no revision with diff. The writer must still fall back to CLI (`start.sh`) to complete any real work. `STRATEGIC_PLAN.md` Phase 3 fixes this by wiring the full interaction loop.

---

## WHAT TO READ (In Order — Don't Skip For Each Sub-Phase)

**For all Phase 3 steps:**
- `AGENTS.md` Section D (architecture: `AppContext` → `StoryEngine` → `RwkvBackend`)
- `crates/app/src/lib.rs` (core primitive; `AppContext` definition and capabilities)
- `crates/app/src/context.rs` (`AppContext` creation and wiring)
- `crates/app/src/session.rs` (`SessionAgent`, `SessionHandle`)
- `crates/app/src/workspace.rs` (`AppWorkspace`, `Timeline`)

**Sub-phase specific:**
- Phase 3.1 (`AppContext` wire): `desktop_app.rs` header (section 4: `new()` method); `crates/app/src/lib.rs` lines 1-50.
- Phase 3.2 (`interaction.rs` mapping): `crates/agent/src/interaction.rs` (`HumanAction`, `InteractionMode`, `InteractionState`); `crates/ui/src/pacing.rs` (`PacingAction`, `PacingMode`).
- Phase 3.3 (`quality.rs`): `crates/agent/src/quality.rs` (`QualityAnalyzer`, `StoryCritique`, `QualityScore`); `crates/ui/src/chat.rs` (how to add new action).
- Phase 3.4 (`reversibility.rs`): `crates/agent/src/reversibility.rs` (`VersionControl`, `Snapshot`, `ReversibleAction`); `crates/ui/src/markdown_editor.rs` (text editing state).
- Phase 3.5 (E2E): `TASK_01_DESKTOP_WIDGETS.md` (prerequisite milestone confirmation); `roadmap/README.md` (definition of done: test proves human can drive it).

---

## EXACT FILE TARGETS BY SUB-PHASE

### Phase 3.1 — Wire `AppContext` to Desktop (`desktop_app.rs` + `app/src/`)
- **Read:** `desktop_app.rs` (lines 47-115: `new()`; lines 700-850: session handling); `crates/app/src/lib.rs` (full file — small, 110 lines).
- **Edit:** `desktop_app.rs` (add `AppContext` creation in `new()`; wire to widget handlers); `crates/app/src/lib.rs` ONLY if missing export found (`AppError` definitions — verify before editing).

### Phase 3.2 — Interactive Mode with Pacing (`interaction.rs` logic → desktop UI)
- **Read:** `crates/agent/src/interaction.rs` (full — defines pace modes); `crates/ui/src/pacing.rs` (maps to desktop events).
- **Edit:** `desktop_app.rs` (map `PacingAction` events to `InteractionState` changes; wire `ChatAction::SendMessage` to generation stream); possibly `crates/ui/src/pacing.rs` (if new mode mapping needed — only if research confirms gap).

### Phase 3.3 — Quality Feedback Integration (`quality.rs` + desktop UI)
- **Read:** `crates/agent/src/quality.rs` (full); `desktop_app.rs` (lines 200-450: action handlers — see how new actions can be added).
- **Edit:** `desktop_app.rs` (add quality check action to `handle_chat_action()` or new menu item); possibly `crates/ui/src/chat.rs` (if quality result display needs new widget state).

### Phase 3.4 — Revision with Diff (`reversibility.rs` + editor)
- **Read:** `crates/agent/src/reversibility.rs` (full); `crates/ui/src/markdown_editor.rs` (how text state works).
- **Edit:** `markdown_editor.rs` (add diff rendering mode or section comparison); `desktop_app.rs` (wire revision action from quality to editor).

### Phase 3.5 — End-to-End Desktop Pipeline Test
- **Edit:** Create `tests/desktop_e2e.rs` (new file — not existing); possibly `crates/ui/src/lib.rs` (if new test module needs exports).
- **Do NOT edit:** `desktop_app.rs` for this phase (test uses existing wired desktop, not new features).

---

## STEP-BY-STEP PROCEDURE

### Phase 3.1 — Wire `AppContext` to Desktop

**Step 1. Read `AppContext` definition.**
```bash
cat crates/app/src/lib.rs
```
Look for: `pub use context::AppContext;` and the `AppError` enum. Confirm `AppContext` exists and has `session_agent_message`, `generate_stream`, `workspace_transform`, etc.

**Step 2. Read desktop `new()` method.**
```bash
sed -n '80,130p' crates/ui/src/desktop_app.rs
```
Look for: `pub fn new(backend: Option<Arc<dyn ModelBackend>>) -> Self`. Confirm backend parameter exists (added in previous desktop work). Confirm `AppContext` is NOT yet created here.

**Step 3. Create `AppContext` in desktop.**
In `desktop_app.rs`, inside `new()` method (after `Self { ... }` initialization, before return), add:
```rust
// Create the surface primitive connecting desktop to engine
let app_context = AppContext::new();
```
**If `AppContext::new()` doesn't exist:** Read `crates/app/src/context.rs` (`pub struct AppContext`). If it uses `from_env()` or takes parameters, use the actual constructor. If missing entirely (should exist per `AGENTS.md` Section D architecture diagram), ask user — this indicates engine-level gap.

**Step 4. Pass context to widget handlers.**
Modify `handle_chat_action()` and other action handlers to use `self.app_context`. If `self.app_context` field doesn't exist in `RocoDesktopApp`, add it to the struct definition (line ~85):
```rust
app_context: AppContext,
```
Then initialize in `Self { ... }`: `app_context: AppContext::new(),`

**If adding field breaks `Self` initialization:** Read `Self { ... }` block in `new()` (lines 100-115). Add `app_context: AppContext::new(),` to the initialization list. If other fields depend on `AppContext`, add initialization order accordingly.

**Step 5. Wire workspace creation through `AppContext`.**
In `new_session()`, instead of direct `workspace_dir` creation, use:
```rust
self.app_context.workspace().create_workspace();
```
Only if `AppContext` has workspace capability (`AGENTS.md` Section D: `workspace` capability namespace). Check `crates/app/src/workspace.rs` if needed.

**If `AppContext` lacks workspace access:** The architecture diagram (`AGENTS.md` D.1) says `workspace` is a shared capability. If missing, document the gap and ask user — do NOT invent `AppContext` methods.

**Milestone 3.1:** `run_desktop.sh` launches; `AppContext` created without panic; `new_session()` creates workspace directory (verify `.roco/sessions/` or `.roco/workspaces/` appears when clicking "New Session").

---

### Phase 3.2 — Interactive Mode with Pacing

**Step 1. Confirm `interaction.rs` state.**
```bash
grep -n "InteractionMode\|HumanAction\|InteractionState" crates/agent/src/interaction.rs | head -n 20
```
Look for: `InteractionMode::FullControl`, `ModerateControl`, `NoControl`, `GoHam` (or `AutoAccept` — verify exact names).

**Step 2. Confirm desktop `PacingMode` mapping.**
```bash
grep -n "PacingMode" crates/ui/src/pacing.rs
```
Look for: enum variants. Confirm mapping: `PacingMode::Careful` = `FullControl`? `PacingMode::AutoAccept` = `GoHam`? Check `AGENTS.md` Section D architecture for intended mapping.

**Step 3. Map `PacingAction` to interaction changes.**
In `desktop_app.rs`, find `handle_chat_action()` or add new `handle_pacing_action()` method (if separate handler exists for pacing). Modify to set interaction state:
```rust
self.app_context.set_interaction_mode(match action {
    PacingAction::FullControl => InteractionMode::FullControl,
    PacingAction::AutoAccept => InteractionMode::GoHam, // or AutoAccept if that's the name
    // ... etc
});
```
**If `AppContext::set_interaction_mode()` doesn't exist:** Read `crates/app/src/lib.rs` capabilities. If `interaction_mode` isn't a capability, the interaction logic may need to be called directly through `StoryEngine` (`crates/agent/src/story_engine.rs` `StoryConfig::interactive`). Check `story_engine.rs` `StoryConfig` definition. If interaction mode is handled at engine level (not app level), wire through `StoryEngine::new()` or `engine.interaction_state_mut()` instead of `AppContext`.

**Step 4. Wire generation through `AppContext` stream.**
In `handle_chat_action()` for `ChatAction::SendMessage`, instead of direct backend call (if any exists), use:
```rust
self.app_context.generate_stream(&request, |token| {
    // Update chat message with streamed token
    self.chat_state.append_token(token);
});
```
Only if `AppContext::generate_stream()` exists (`AGENTS.md` Section D: `generate_stream` capability). Check `crates/app/src/lib.rs`.

**If `generate_stream()` missing:** Check `crates/app/src/lib.rs` capabilities list. If `generate_stream` is listed but method missing, ask user. If not listed, the desktop should call backend directly (`Arc<dyn ModelBackend>::complete()`) — but this requires backend access in desktop (`desktop_app.rs` `new()` takes `Option<Arc<dyn ModelBackend>>`). Use the existing backend parameter if present. If desktop doesn't have backend yet (`None`), use dummy generation for this phase (focus is interaction flow, not model output).

**Milestone 3.2:** User selects `Careful` pacing; desktop shows outline generation step individually; selects `AutoAccept`; desktop advances automatically through steps. No crashes.

---

### Phase 3.3 — Quality Feedback Integration

**Step 1. Read quality analysis structure.**
```bash
cat crates/agent/src/quality.rs | head -n 100
```
Look for: `QualityAnalyzer::new()`, `evaluate_chapter()`, `StoryCritique` fields (`scores`, `should_revise`, `priority_revisions`, `summary`).

**Step 2. Read desktop chat actions.**
```bash
grep -n "ChatAction::" crates/ui/src/chat.rs | head -n 15
```
Look for existing actions (`SendMessage`, `Accept`, `Skip`, `Stop`, `Clear`, `Undo`, `Retry`).

**Step 3. Add quality action.**
In `desktop_app.rs`, add to `handle_chat_action()` (or create `handle_quality_action()` if separate):
```rust
// When user clicks quality check (add button or command)
let critique = self.app_context.evaluate_quality(chapter_num);
self.status_message = format!("Quality: {:.1}/10 - {} revisions needed",
    critique.scores.overall,
    if critique.should_revise { "Some" } else { "None" });
// Show critique details in right panel or overlay
```
Only add exact methods that exist in `AppContext` or `StoryEngine`. Check `crates/app/src/lib.rs` and `story_engine.rs` `evaluate_chapter_quality()`.

**If `AppContext` doesn't expose quality evaluation:** Call through `StoryEngine` directly. In desktop `new()`, create a `StoryEngine` (check `story_engine.rs` `StoryEngine::new()` constructor — takes `StoryConfig`). Use `engine.generate_chapter()` then `engine.evaluate_chapter_quality()`.

**Step 4. Display results.** Modify `show_right_panel()` or add overlay in `desktop_app.rs`: when right panel shows `Editor`, also show quality scores at top or in a tooltip/status area.

**Milestone 3.3:** User can trigger quality check; desktop shows `StoryCritique` results (overall score + revision suggestions) without crash.

---

### Phase 3.4 — Revision with Diff

**Step 1. Confirm reversibility functions.**
```bash
grep -n "ReversibleAction\|Snapshot\|VersionControl" crates/agent/src/reversibility.rs | head -n 20
```
Look for: how to create snapshot (`VersionControl::snapshot()`?), how to compare/revise (`ReversibleAction::apply()`?).

**Step 2. Confirm editor state.**
```bash
grep -n "MarkdownEditorState\|document" crates/ui/src/markdown_editor.rs | head -n 10
```
Confirm text storage mechanism (`document.text` or similar).

**Step 3. Implement diff display.** In `markdown_editor.rs`, add a `show_diff()` mode or add a new widget state field (`show_diff: bool`). When `show_diff` is true, display original text (from `self.chapters[chapter_num - 1]` or workspace file `03-CHAPTER_X.md`) and revised text side by side or stacked.

**If `MarkdownEditor` state doesn't support dual-view:** Add `original_text: String` and `revised_text: String` to `MarkdownEditorState`. Set these when revision starts. Render original at top, revised at bottom, with visual separator.

**Step 4. Wire revision from desktop.** In `desktop_app.rs`, add to `handle_chat_action()` or new menu: when user selects "Revise" (after quality check shows `should_revise: true`), load original chapter into `original_text`, run `engine.revise_chapter()`, load result into `revised_text`, show diff.

**If `AppContext` or desktop doesn't have direct `revise_chapter()` access:** Use `StoryEngine::revise_chapter()` directly (check constructor). The desktop `new()` method may need to create `StoryEngine` (check `story_engine.rs` for constructor requirements — `StoryConfig` needed).

**Milestone 3.4:** User sees original vs revised chapter; can accept full revision or reject; workspace file updates if accepted.

---

### Phase 3.5 — End-to-End Desktop Pipeline Test (`tests/desktop_e2e.rs`)

**Step 1. Create new test file.**
```bash
touch crates/ui/src/tests/desktop_e2e.rs
mkdir -p crates/ui/src/tests 2>/dev/null || echo "tests directory may already exist"
```
**Note:** `tests/` directory may need to be under crate root (`crates/ui/src/` or `crates/ui/tests/`). Check `Cargo.toml` for `roco-ui` to see test structure. If `Cargo.toml` has `[[test]]` sections, follow that. Otherwise, `src/tests/` or `tests/` at crate root works with `cargo test -p roco-ui`.

**Step 2. Write integration test.**
```rust
#[test]
fn test_full_story_pipeline_in_desktop() {
    // 1. Create desktop app with dummy backend (or mock if available)
    // 2. Call new_session() or equivalent
    // 3. Send message with premise: "Write a short fantasy"
    // 4. Verify workspace directory created (.roco/workspaces/)
    // 5. Verify outline file exists (01-OUTLINE.md) after outline generation
    // 6. Verify at least 1 chapter file exists (03-CHAPTER_1.md) after generation
    // 7. Verify complete story exists (06-STORY.md) after publish action
    // Note: This test may hang if it tries to create real RwkvBackend.
    // Use MockBackend from engine crate if available (`roco_engine::MockBackend`).
    // Check `crates/engine/src/backend.rs` for `MockBackend` definition.
}
```

**If test tries to create real backend and hangs:** Check `crates/engine/src/backend.rs`. If `MockBackend` exists, use it: `MockBackend::default()` (or similar constructor). Pass to desktop `new()` as `Some(Arc::new(MockBackend::default()))`. The desktop `new()` accepts `Option<Arc<dyn ModelBackend>>` — confirm with `grep -n "fn new" crates/ui/src/desktop_app.rs`.

**Step 3. Verify workspace file contents.**
In test, after pipeline completes, read `workspace_path.join("01-OUTLINE.md")` and verify it contains "Title:" or "Chapter" (outline format from `story_engine.rs` `render_outline()` method). Read `workspace_path.join("06-STORY.md")` and verify non-empty and contains "# " (story title).

**Step 4. Run test.**
```bash
cargo test -p roco-ui -- desktop_e2e --nocapture -q
```
**If hangs:** The test likely tries to load a real RWKV model. Confirm `MockBackend` is used. If `MockBackend` returns empty responses, the pipeline may fail differently (parse errors). That's expected — the milestone is that the desktop completes the pipeline steps (workspace created, outline saved, chapter saved) without UI crashes. The test proves the human can drive it through desktop.

**Milestone 3.5:** `tests/desktop_e2e.rs` exists; runs without desktop crash; verifies workspace output files. This is the Definition of Done for Phase 3.

---

### TROUBLESHOOTING FOR TASK 02 (If Any Phase Fails)

| Phase | Failure Pattern | Immediate Check | Fix Reference |
|---|---|---|---|
| 3.1 AppContext wire | `AppContext` missing method | `cat crates/app/src/lib.rs` — check `pub use` exports; check capability comments | `crates/app/src/lib.rs` lines 15-35 |
| 3.1 Workspace not created | `.roco/workspaces/` missing after `New Session` | Check `AppContext::workspace()` return; check `Workspace::from_existing()` in `workspace.rs` | `crates/app/src/workspace.rs` |
| 3.2 Pacing mode crash | `PacingAction` variant not handled | `grep -n "PacingAction::" crates/ui/src/desktop_app.rs` — confirm all variants mapped | `pacing.rs` enum definition |
| 3.2 Generation hangs | `.await` in `update()` or backend creates real RWKV load | Check `desktop_app.rs` for `.await` (should use `block_on`); confirm backend is `None` or `MockBackend` | `AGENTS.md` Section J.3 |
| 3.3 Quality crash | `StoryEngine::evaluate_chapter_quality()` missing or wrong args | `grep -n "evaluate_chapter_quality" crates/agent/src/story_engine.rs` — check signature (`&self`, `backend`, `chapter_num`) | `quality.rs` lines 1-50 |
| 3.4 Revision fails | `MarkdownEditor` has no `original_text` storage | Add fields to `MarkdownEditorState`; confirm constructor accepts them | `markdown_editor.rs` struct definition |
| 3.5 E2E test hangs | Real model load in test | Confirm `MockBackend` used (`grep -n "MockBackend" crates/engine/src/backend.rs` to verify existence) | `TASK_01_DESKTOP_WIDGETS.md` troubleshooting table |

---

*This task file references `AGENTS.md` Sections A, C, D, E, G, H; `EDIT_GUIDE.md`; `PROJECT_STRUCTURE.md`; `STRATEGIC_PLAN.md` Phase 3; `TASK_01_DESKTOP_WIDGETS.md` (prerequisite).*
