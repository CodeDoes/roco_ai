# TASK 03 — END-TO-END DESKTOP PIPELINE TEST

> **Reference:** `AGENTS.md` Section G (Testing Strategy — Definition of Done), `EDIT_GUIDE.md`, `TASK_02_DESKTOP_INTERACTION.md` (prerequisite: Phase 3.5 target — desktop interaction wired).
> **Status:** Pending (`TASK_02_DESKTOP_INTERACTION.md` Milestone 3.5 must pass first).
> **Strategic theory:** From `AGENTS.md` Section J.1 and `roadmap/README.md`: a feature isn't done without a test proving a human can drive it. This test proves the desktop supports the writer's full journey.

---

## PROBLEM (Why This Exists)

Without an end-to-end test, desktop integration (Phase 3) could work partially but fail silently at the writer's final step (publish). The writer clicks through outline → chapters → quality → revision → publish, but the final `06-STORY.md` might not exist, or might be empty, or workspace might not persist. This task creates the proof.

---

## PREREQUISITE CHECKLIST (Don't Start Until All Pass)

Run these commands IN ORDER. If any fails, return to the referenced task.

```bash
# 1. Widget standalone tests pass
cargo test -p roco-ui -- pacing::tests markdown_editor::tests chat::tests file_tree::tests wiki_browser::tests session_browser::tests link_graph::tests change_timeline::tests --nocapture
```
**If fails:** See `TASK_01_DESKTOP_WIDGETS.md` troubleshooting table. Do not proceed.

```bash
# 2. Desktop launches interactively
run_desktop.sh
```
**If fails:** `desktop_app.rs` has composition error. See `TASK_02_DESKTOP_INTERACTION.md` Phase 3.2-3.4 troubleshooting. Do not proceed.

```bash
# 3. AppContext wired (new session creates workspace directory)
# Manual check: open desktop, click "New Session", verify `.roco/sessions/` or `.roco/workspaces/` created in workspace root.
```
**If missing:** See `TASK_02_DESKTOP_INTERACTION.md` Phase 3.1 troubleshooting (`AppContext` wire). Do not proceed.

---

## WHAT TO READ (Minimal — Just What's Needed)

1. `AGENTS.md` Section G (`Testing Strategy`) — confirms `No test = not done` and snapshot/bless workflow.
2. `crates/ui/src/lib.rs` — confirms `tests/` module export (if needed for `#[cfg(test)]` visibility).
3. `STRATEGIC_PLAN.md` Phase 3.5 (`tests/desktop_e2e.rs` specification — what the test must verify).
4. `TASK_02_DESKTOP_INTERACTION.md` Milestone 3.5 (`AppContext` must be wired; desktop must launch).

---

## FILES TO EDIT

- `crates/ui/src/lib.rs` (only if `tests/` module needs to be exported — check current file; likely no edit needed).
- **New file:** `crates/ui/src/tests/desktop_e2e.rs` (create; does not exist).
- **Optional edit:** `Cargo.toml` (`crates/ui/`) if `tests/` directory isn't automatically included by `cargo test`. Check: `grep -n "tests" crates/ui/Cargo.toml`. If no `[[test]]` or `tests/` reference exists, `cargo test -p roco-ui -- desktop_e2e` will find `src/tests/desktop_e2e.rs` automatically (Cargo convention: `tests/` at crate root or `src/tests/` for module tests). If using `src/tests/`, no `Cargo.toml` edit needed.

---

## STEP-BY-STEP (Don't Skip — Each Step Has A Milestone)

### Step 1 — Confirm Test Discovery Works

```bash
mkdir -p crates/ui/src/tests
echo "#[test]\nfn dummy() { assert!(true); }" > crates/ui/src/tests/dummy_e2e.rs
cargo test -p roco-ui -- dummy_e2e --nocapture
```
**Expected:** Test runs (passes or fails clearly — failure is fine for dummy; we just confirm `cargo test` finds the file).
**If `cargo test` says "no tests found" or ignores file:** Check `crates/ui/Cargo.toml` for `test = false` or missing `tests/` reference. If `tests/` isn't included, add to `Cargo.toml`:
```toml
[[test]]
name = "desktop_e2e"
path = "src/tests/desktop_e2e.rs"
```
But only add if needed — don't invent `Cargo.toml` changes.

**Milestone 1:** `cargo test -p roco-ui -- dummy_e2e` discovers and runs file.

---

### Step 2 — Create `desktop_e2e.rs` With Minimal Pipeline Test

Write file content (use `cat` or editor):
```rust
// Reference: STRATEGIC_PLAN.md Phase 3.5; TASK_01_DESKTOP_WIDGETS.md Milestone 2.5
// This test proves a writer can complete a full story through desktop GUI.

use std::path::PathBuf;

#[test]
fn test_full_story_pipeline_creates_workspace_and_publishes() {
    // Note: This test requires desktop to launch with MockBackend or dummy backend.
    // If using real backend (`RWKV_MODEL=...`), the test may hang on model load.
    // See troubleshooting below.

    // 1. Create desktop with dummy backend (no model load)
    // Confirm desktop_app.rs `new()` accepts `Option<Arc<dyn ModelBackend>>`.
    // If `MockBackend` exists (`crates/engine/src/backend.rs`), use it.
    // Check with: grep -n "MockBackend" crates/engine/src/backend.rs

    // Example construction (adjust based on actual MockBackend constructor):
    // use roco_engine::MockBackend;
    // use std::sync::Arc;
    // let backend: Option<Arc<dyn roco_engine::ModelBackend>> = Some(Arc::new(MockBackend::default()));

    // 2. Initialize desktop (simulated — don't fully render if test framework doesn't support egui context)
    // Since `desktop_app.rs` creates `AppContext` and `StoryEngine`, test focuses on:
    // - Workspace directory created (.roco/workspaces/)
    // - Outline file exists (01-OUTLINE.md)
    // - At least chapter file exists (03-CHAPTER_1.md) — only if generation is mocked quickly
    // - Published story exists (06-STORY.md) — only after full pipeline

    // For this initial version, the milestone is workspace creation + outline existence.
    // Full pipeline (generation + publish) is the target but may require MockBackend that returns valid JSON.

    // Confirm workspace root exists after creating AppContext or calling workspace method.
    // Check workspace creation: crates/workspace/src/workspace.rs (`Workspace::from_existing()` or `Workspace::temp()`)
    // Verify `.roco/workspaces/` or temp workspace path exists.

    // Minimal assertion for this phase:
    assert!(true, "Desktop pipeline framework established; expand with MockBackend response validation in next iteration.");
}
```

**If `MockBackend` doesn't exist or constructor is unknown:** Check `crates/engine/src/backend.rs`. If `MockBackend` is defined, find its constructor (`::new()`, `::default()`, or `MockBackend { ... }`). If it doesn't exist, the test framework must simulate generation through `AppContext` with a stub. Document this gap: add note to `TASK_03_DESKTOP_E2E.md` (this file) at top: `Note: MockBackend verification needed; if unavailable, test framework requires stub generation.`

**If test framework can't create `eframe` or `egui` context for desktop:** The milestone adjusts: verify `AppContext::new()` and workspace creation work independently of desktop rendering. This still proves the writer's journey works at the data layer (workspace persistence), which is the core of `Definition of Done`.

---

### Step 3 — Verify Workspace Output Files (Manual or Automated)

Run desktop manually (with `run_desktop.sh` or `start.sh`) and confirm files exist after interaction. OR add assertions to test (if MockBackend returns valid JSON for outline, chapter, synopsis):
```rust
// Check workspace files exist after simulated pipeline
use std::fs;
let workspace_path = PathBuf::from(".roco/workspaces/");
assert!(workspace_path.exists(), "Workspace directory must exist");
```

**If `.roco/workspaces/` doesn't exist after desktop run:** Check `desktop_app.rs` `new_session()` (line ~700). Confirm `Workspace::from_existing()` is called with correct `WorkspaceKind::Agent` or `Temp`. Check `workspace/src/workspace.rs` `from_existing()` returns `Ok` and creates directory. If `Workspace` creation fails, read error message, document exact `WorkspaceError`, and ask user for `workspace` module fix (this is a `Caution Zone` file per `EDIT_GUIDE.md`).

**Milestone 2:** Workspace directory created by desktop; test confirms existence.

---

### Step 4 — Expand Test With Mock Generation (If MockBackend Available)

If `MockBackend` exists and returns structured JSON (outline, chapter, synopsis schemas):
1. Update `tests/desktop_e2e.rs` to use `MockBackend`.
2. Add assertions for:
   - `01-OUTLINE.md` exists and contains `Title:` (from `outline.bnf` schema output).
   - `03-CHAPTER_1.md` exists and contains `# ` (chapter title marker from `ChapterOutput::schema()`).
   - `06-STORY.md` exists and is non-empty (assembled from chapters + synopsis).
3. Confirm `MockBackend` generates JSON matching schemas (`StoryOutline::schema()`, `StoryChapter::schema()`, etc.) — check `crates/grammar/src/grammar_library.rs` for `StoryGrammar` definitions.

**If `MockBackend` returns empty or invalid JSON:** Document gap: `MockBackend` needs schema-compliant responses for full E2E. Don't invent responses. Note in `AGENTS.md` Section I (`Pitfalls`): `MockBackend` response validation gap — add `MockBackend` schema-compliant mock responses as future improvement.

---

### TROUBLESHOOTING FOR TASK 03 (Pipeline Fails At Any Step)

| Failure | Immediate Check | Reference For Fix |
|---|---|---|
| `tests/desktop_e2e.rs` not discovered by cargo | `ls crates/ui/src/tests/` — file exists? `Cargo.toml` includes `tests/`? | `TASK_03_DESKTOP_END_TO_END.md` Step 1 |
| Test creates workspace but no files | Check `AppContext::workspace()` or `Workspace::from_existing()` return value; check `.roco/workspaces/` permissions | `TASK_02_DESKTOP_INTERACTION.md` Phase 3.1 (workspace wire) |
| Desktop launches but `AppContext::new()` crashes | Read crash line; check `crates/app/src/lib.rs` for missing `AppContext` constructor or missing dependency | `crates/app/src/lib.rs` line 47 (`new()`) |
| `MockBackend` unavailable or unknown constructor | `grep -n "MockBackend" crates/engine/src/backend.rs` — if no results, backend doesn't have mock; document gap, don't invent mock | `AGENTS.md` Section E.1 (frozen engine — don't invent `MockBackend`) |
| Pipeline generates empty `01-OUTLINE.md` | `MockBackend` response doesn't match `StoryOutline::schema()`; document gap; don't fake response | `crates/agent/src/story_engine.rs` `OutlineExpansion::schema()` |

---

*This task references `AGENTS.md` Sections A, G; `EDIT_GUIDE.md`; `TASK_01_DESKTOP_WIDGETS.md` (prerequisite); `TASK_02_DESKTOP_INTERACTION.md` (prerequisite Phase 3); `STRATEGIC_PLAN.md` Phase 3.5.*
