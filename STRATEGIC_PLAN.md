# STRATEGIC DEVELOPMENT PLAN — RoCo AI

> **Purpose:** This document exists so the agent never needs to research direction independently. Every strategic decision, milestone, and file target is written here based on synthesized research (see `AGENTS.md` Section 8: Research Synthesis).
>
> **Status:** Living document. Update when milestones are reached or when new evidence changes direction.
>
> **Created:** 2026-07-20 (post-AGENTS.md 2.0 rewrite)

---

## PART A — STRATEGIC THEORY (Why This Direction)

### A.1 The Core Problem

The repo has a correct, frozen engine (`crates/inference/`, `engine/`, `grammar/`, `agent/`) but an untested, fragmented user experience:
- CLI (`start.sh`) works but requires manual model setup and terminal comfort.
- Web apps (`apps/chat/`, `apps/studio/`, `apps/editor/`) have zero tests and require a separate `npm` build + `roco-server` HTTP layer.
- Desktop (`crates/ui/`) exists but is untested as a composed application.
- There is no single surface where all control flows (pace modes, outline editing, quality check, revision, persistence) are visible and driven by a real user.

This is exactly the gap identified in `AGENTS.md` (original 2026-07-19): **"No tested human surface."**

### A.2 The Strategic Hypothesis

Based on multi-agent framework studies (`StoryEnsemble`, `PlayWrite`) and desktop UI best practices (`egui` core team + production apps), the best path is:

**Desktop-first (`egui`) + CLI headless (`start.sh`) = primary surfaces.**
Web apps become secondary (plugin targets, not primary UX). The desktop provides:
- Direct `RwkvBackend` access (no HTTP overhead)
- Same Rust test framework as engine
- Multi-panel layout (pacing, chat, editor, browsers, timeline) matching writer workflow studies
- Immediate-mode rendering = simpler state management than reactive web frameworks

### A.3 Why Not Web-First?

| Factor | Desktop (`egui`) | Web (`Next.js` / `Vite`) |
|---|---|---|
| **Integration with engine** | Direct `Arc<dyn ModelBackend>` | Requires `roco-server` + HTTP + serialization |
| **Testing** | `cargo test` (same framework) | `npm test` (separate, no shared assertions) |
| **State persistence** | Direct `Workspace` access | Must serialize through server endpoints |
| **Performance** | 60 FPS debug; no browser overhead | Browser + server latency |
| **Build step** | `cargo build --release` (single) | `cargo build` + `npm install` + `npm run dev` + server start (multiple) |
| **Migration cost** | Move widget logic from web to Rust (structured) | Keep two separate surfaces forever (fragmented) |

**Decision (confirmed 2026-07-19, `roadmap/blocked.md`):** `egui` chosen over rejected `gpui`. Desktop is the structural fix.

### A.4 The Writer's Experience Model (Based on Research)

From `PlayWrite` (2018) and `StoryEnsemble` (2025):
- Writers treat AI as collaborative partner when they can **directly manipulate** story elements (timeline, outline nodes, character links) rather than only typing text.
- **Writer-Editor loop** improves quality: generate → evaluate (model-as-judge) → revise based on structured feedback. Our `story_engine.rs` implements this (`generate_chapter()` → `evaluate_chapter_quality()` → `revise_chapter()`).
- **Dynamic exploration** beats linear pipeline. Writers jump between outline, plot state, and chapters freely. Our desktop `desktop_app.rs` provides `RightPanelTool` browsers (`FileTree`, `Wiki`, `LinkGraph`, `Sessions`, `Timeline`) for exactly this.
- **Pace control** must be visible. Hidden approval gates create friction. `PacingWidget` (`PacingMode::Planning` / `Careful` / `Rolling` / `AutoAccept`) exposes this directly.

### A.5 The Migration Strategy (Web → Desktop)

Not "rewrite everything." Not "delete web apps immediately." Structured migration:

1. **Phase 2** (Weeks 1-3): Desktop widgets work standalone (tests pass). Web apps untouched.
2. **Phase 3** (Weeks 4-6): Desktop supports full interaction flow (outline → chapter → quality → publish). Web apps remain available but deprecated for new features.
3. **Phase 4** (Weeks 7-8): Desktop absorbs web editor (`MarkdownEditor` gets rich text + file tree) and web chat (`ChatWidget` gets streaming + markdown). Web apps frozen (bug fixes only, no new features).
4. **Phase 5** (Week 9+): Plugins (`vscode/`, `zed/`, `obsidian/`) verified against desktop or server. `API.md` expanded if server endpoints change.

---

## PART B — DETAILED DEVELOPMENT PLAN (Step-by-Step)

Each phase has: **Files to Read**, **Files to Edit**, **Milestones** (testable exit criteria), **Strategic Note** (why this step matters for the writer's experience).

---

### PHASE 1 — Agent Safety & Documentation (COMPLETED 2026-07-20)
**Status:** ✅ Completed
**Strategic result:** Any agent can read this file and `EDIT_GUIDE.md` and work safely. Any writer can run `./start.sh`.

---

### PHASE 2 — Desktop Widget Standalone-First Build (NEXT — Target: 3 Weeks)
**Strategic result:** Every desktop widget works independently with tests. Composition bugs become layout-only, not logic bugs.

**Phase 2.1 — PacingWidget Tests (Target: Week 1)**
- **Files to read:** `crates/ui/src/pacing.rs`, `crates/ui/src/lib.rs`
- **Files to edit:** `crates/ui/src/pacing.rs` (add `#[cfg(test)]` module at bottom)
- **What to test:** `PacingMode` ↔ `InteractionMode` conversion (`PacingWidgetState::new()`), event emission (`PacingAction` variants: `Accept`, `Skip`, `Stop`, `Undo`, `GoHam`, `FullControl`), state persistence.
- **Milestone:** `cargo test -p roco-ui -- pacing::tests` passes. Widget renders without panic.
- **Strategic note:** Pacing is the writer's primary control mechanism. If it fails, the writer loses pace control — violating core philosophy (`AGENTS.md` Section 1, priority 4).
- **Do not touch:** `desktop_app.rs` (don't wire yet).

**Phase 2.2 — MarkdownEditor Tests (Target: Week 1-2)**
- **Files to read:** `crates/ui/src/markdown_editor.rs` (~1230 lines — largest widget)
- **Files to edit:** Add `#[cfg(test)]` at bottom; test basic text input, markdown rendering, save/load through `Workspace`.
- **Milestone:** Editor renders text; basic input handled; workspace file read/write works.
- **Strategic note:** Editor is the writer's primary surface for text. It must work before any composition.

**Phase 2.3 — ChatWidget Tests (Target: Week 2)**
- **Files to read:** `crates/ui/src/chat.rs`
- **Files to edit:** Test message roles (`System`, `User`, `Assistant`, `Event`), message clearing, scroll behavior.
- **Milestone:** `ChatWidget::show()` renders without panic; messages persist.
- **Strategic note:** Chat is the conversation surface. The writer uses it for feedback (`f` command in CLI). Desktop chat must support this.

**Phase 2.4 — Browser Widget Tests (Target: Week 2-3)**
- **Files to read/edit:** `file_tree.rs`, `wiki_browser.rs`, `session_browser.rs`, `link_graph.rs`, `change_timeline.rs` — each gets `#[cfg(test)]` module.
- **Tests:** Core state operations (`FileTreeState::refresh()`, `LinkGraphState::add_node()`, `SessionBrowserState::refresh()`, `WikiBrowserState` page selection, `ChangeTimelineState` entry management).
- **Milestone:** All browser widgets pass basic state tests.
- **Strategic note:** Browsers enable dynamic exploration (`StoryEnsemble` principle). Without them, the writer is locked in a linear pipeline.

**Phase 2.5 — Desktop Integration (Target: Week 3)**
- **Files to edit:** `crates/ui/src/desktop_app.rs` (composition only — after all standalone tests pass)
- **What to wire:** `PacingWidget` (left), `ChatWidget` (center), `MarkdownEditor` + browsers (right via `RightPanelTool`). Menu bar (`File`, `View`, `Help`). Status bar.
- **What NOT to wire:** Model-backed generation (keep `backend: None`; use dummy messages). Focus is UI composition only.
- **Milestone:** `cargo run --release -p roco-ui --bin roco-desktop` launches without panic. All panels visible and interactive (click, type, scroll, toggle right-panel tools).
- **Do not add:** New features. Composition only.

---

### PHASE 3 — Full Interaction Flow + End-to-End Test (Target: 6 Weeks)
**Strategic result:** The desktop supports the complete writer journey: direction → outline → edit → chapters → feedback → quality → revision → publish — with tests proving it.

**Phase 3.1 — Wire AppContext to Desktop (COMPLETED 2026-07-20)**
- **Files to read:** `crates/app/src/lib.rs`, `context.rs`, `session.rs`, `workspace.rs`
- **Files to edit:** `desktop_app.rs` — create `AppContext` in `new()`; pass to widget handlers.
- **What to wire:** `AppContext::generate_stream()` → `ChatWidget` streaming; `workspace_transform()` → `FileTree`; `session_agent_message` → chat persistence.
- **Milestone:** Desktop creates/opens workspace (`.roco/workspaces/`) through UI; file operations visible in `FileTree`.
- **Strategic note:** `AppContext` is the single surface primitive. Without it wired to desktop, the desktop is just a pretty shell.

**Phase 3.2 — Interactive Mode with Pacing (COMPLETED 2026-07-20)**
- **Files to read:** `crates/agent/src/interaction.rs`
- **Files to edit:** `desktop_app.rs` — map `PacingAction` to `InteractionMode`; wire `ChatAction::SendMessage` to generation.
- **What to add:** `PacingMode::Careful` shows outline first, then chapter individually. `AutoAccept` auto-advances through steps. User selects mode in left panel.
- **Milestone:** User types `"Write a dark fantasy"` in chat; desktop shows outline; user can edit outline (via editor opening `01-OUTLINE.md`); first chapter appears.
- **Strategic note:** This is the writer's primary interaction loop (`AGENTS.md` Section 4, Pattern 1). Without it, the desktop is incomplete.

**Phase 3.3 — Quality Feedback Integration (NEXT — Target: Week 5)**
- **Files to read:** `crates/agent/src/quality.rs`
- **Files to edit:** `desktop_app.rs` — add quality check action; display `StoryCritique` results (scores: overall, pacing, show-don't-tell, character voice, plot coherence, engagement) in right panel or overlay.
- **Milestone:** User clicks quality check; sees scores with revision suggestions.
- **Strategic note:** Quality evaluation (`evaluate_chapter_quality()`) is part of the Writer-Editor loop. Without visible quality results, the writer lacks structured feedback.

**Phase 3.4 — Revision with Diff (Target: Week 5-6)**
- **Files to read:** `crates/agent/src/reversibility.rs`
- **Files to edit:** `markdown_editor.rs` — add diff view (original vs revised). `desktop_app.rs` — wire revision from quality results to editor.
- **Milestone:** User revises chapter based on quality feedback; sees diff; can accept/reject per section or full revision.
- **Strategic note:** Revision with visible diff is the writer's control mechanism (`AGENTS.md` Section 4, Pattern 2). Hidden revisions remove writer agency.

**Phase 3.5 — End-to-End Desktop Pipeline Test (Target: Week 6)**
- **Files to edit:** `crates/ui/src/lib.rs` or new `tests/desktop_e2e.rs`
- **What to test:** Create workspace → generate outline → generate 3 chapters → publish → verify `06-STORY.md` exists with content.
- **Milestone:** `cargo test -p roco-ui -- desktop_e2e` passes.
- **Exit criteria for Phase 3 (Definition of Done):** Writer can complete full story pipeline in desktop GUI without using CLI; desktop has standalone widget tests; end-to-end test passes.

---

### PHASE 4 — Web-to-Desktop Migration (Target: 8 Weeks)
**Strategic result:** Web apps become secondary; desktop is the tested, integrated primary surface.

**Phase 4.1 — Editor Migration (Target: Week 7)**
- **Files to read:** `apps/editor/src/main.ts`, `api.ts`
- **Files to edit:** `markdown_editor.rs` — rich text editing (`egui_markdown`), file tree integration, workspace auto-save, optional direct `RwkvBackend` connection (no HTTP needed for core).
- **Decision confirmed:** Desktop uses direct backend, not server HTTP. `roco-server` remains for plugins/external use.
- **Milestone:** User opens `01-OUTLINE.md` in desktop editor; edits; saves; sees file in workspace directory.

**Phase 4.2 — Chat Migration (Target: Week 7)**
- **Files to read:** `apps/chat/components/chat.tsx`, `page.tsx`
- **Files to edit:** `chat.rs` — streaming messages, markdown formatting, responsive layout adjustments.
- **Milestone:** Chat renders streamed assistant messages; markdown works.

**Phase 4.3 — Studio Integration (Target: Week 8)**
- **Files to read:** `apps/studio/components/*.tsx`
- **Files to edit:** `desktop_app.rs` — compose existing widgets into unified interface (`FileTree` + `Wiki` + `LinkGraph` + `Sessions` + `Timeline` + `Editor` + `Chat`).
- **Milestone:** Desktop shows unified studio: left (pacing + session info), center (chat + generation), right (editor + browsers + timeline). All interactive.

**Phase 4.4 — Freeze Web Apps (Concurrent with 4.1-4.3)**
- **Policy:** `apps/chat/`, `apps/studio/`, `apps/editor/` frozen for new features. Bug fixes allowed. New features go to `crates/ui/`.
- **Milestone:** `README.md` and `PROJECT_STRUCTURE.md` clearly state desktop is primary; web apps deprecated for new development.

---

### PHASE 5 — Plugin & API Verification (Target: 9+ Weeks)
**Strategic result:** Plugins work with desktop backend or server; API docs accurate.

**Phase 5.1 — VSCode Plugin Verification (Target: Week 9)**
- **Files to read/edit:** `apps/plugins/vscode/src/extension.ts` (only if broken)
- **Milestone:** Plugin connects to `roco-server`; `RoCo: Generate Chapter` executes successfully.

**Phase 5.2 — Zed + Obsidian Plugin Verification (Target: Week 9)**
- **Files:** `apps/plugins/zed/src/lib.rs`, `extension.toml`; `apps/plugins/obsidian/main.ts`
- **Milestone:** Plugins documented (`PLUGINS.md` already exists); basic functionality verified or documented as limited.

**Phase 5.3 — API Documentation Update (Target: Week 10)**
- **Files to read/edit:** `API.md` — expand if desktop backend introduces new endpoints or if server behavior changes due to desktop-first migration.
- **Milestone:** API docs match actual server behavior when desktop connects via `RemoteBackend`.

---

### PHASE 6 — Continuous Improvement (Ongoing)
**Strategic result:** The repo stays green, the experience improves incrementally, and the agent never needs to research independently.

**Ongoing Rules:**
- **Every change updates `roadmap/progress.md`** (append-only, dated, with done/not-done status).
- **Every new feature adds a test** (unit or integration). No exceptions.
- **Every widget stays standalone-first.** Before wiring into `desktop_app.rs`, the widget passes `cargo test -p roco-ui -- widget_name`.
- **Quarterly review of `AGENTS.md`** (Section 11). Remove stale patterns. Verify file paths in Critical Files.
- **No engine modifications without blocking-feature proof.** Check `EDIT_GUIDE.md` first. Read file header markers.

---

## PART C — FILE TARGETS BY PHASE (Quick Reference)

| Phase | Primary Files | Secondary Files | Milestone Command |
|---|---|---|---|
| 1 (Done) | `AGENTS.md`, `EDIT_GUIDE.md`, `PROJECT_STRUCTURE.md`, `start.sh`, `README.md` | Source markers in large files | `run_tests.sh` passes |
| 2.1 | `crates/ui/src/pacing.rs` | — | `cargo test -p roco-ui -- pacing::tests` |
| 2.2 | `crates/ui/src/markdown_editor.rs` | `crates/ui/src/lib.rs` | Editor test passes |
| 2.3 | `crates/ui/src/chat.rs` | — | Chat widget test passes |
| 2.4 | Browser widget files (`file_tree.rs`, etc.) | — | All browser tests pass |
| 2.5 | `crates/ui/src/desktop_app.rs` | — | `run_desktop.sh` launches, all panels interactive |
| 3.1 | `crates/app/src/lib.rs`, `desktop_app.rs` | `crates/app/src/context.rs` | Desktop opens workspace |
| 3.2 | `desktop_app.rs`, `crates/agent/src/interaction.rs` | — | Interactive mode works |
| 3.3 | `desktop_app.rs`, `crates/agent/src/quality.rs` | — | Quality check visible |
| 3.4 | `markdown_editor.rs`, `reversibility.rs` | — | Diff + revision works |
| 3.5 | `tests/desktop_e2e.rs` (new) | — | End-to-end pipeline passes |
| 4.1 | `markdown_editor.rs`, `apps/editor/` | — | Editor migration complete |
| 4.2 | `chat.rs`, `apps/chat/` | — | Chat migration complete |
| 4.3 | `desktop_app.rs`, `apps/studio/` | — | Studio integration complete |
| 4.4 | `README.md`, `PROJECT_STRUCTURE.md` | — | Web apps frozen |
| 5.1 | `apps/plugins/vscode/src/extension.ts` | `PLUGINS.md` | VSCode plugin verified |
| 5.2 | `apps/plugins/zed/src/lib.rs`, `obsidian/main.ts` | — | Zed + Obsidian verified |
| 5.3 | `API.md` | — | API docs accurate |

---

*This strategic plan is protected content. Updates must reference `AGENTS.md` Section 11 (Maintenance Rules) and include a dated entry in `roadmap/progress.md`.*
