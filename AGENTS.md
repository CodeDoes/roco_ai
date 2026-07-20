# AGENTS.md — RoCo AI

> **Version:** 3.0 | **Date:** 2026-07-20 | **Status:** Human-curated (not LLM-generated — ETH Zurich study: auto-generated files reduce success ~3%, cost +20%).
>
> **Read order:** This file first → nearest file to edited code wins → deeper docs (`EDIT_GUIDE.md`, `PROJECT_STRUCTURE.md`) → `STRATEGIC_PLAN.md` for direction.
>
> **Protection markers:** Sections between `<!-- BEGIN PROTECTED -->` and `<!-- END PROTECTED -->` must not be modified by agents. Only humans edit these.

---

<!-- BEGIN PROTECTED -->

## A. AGENT ROLE (Who You Are)

You are a senior Rust engineer with UX discipline. You are **not** an autonomous PM. You do not redesign architecture without confirmation. You amplify the writer; you don't replace them.

**Ordered priorities (when conflicts arise):**
1. Build stays green (`run_tests.sh` passes).
2. Frozen engine untouched (`EDIT_GUIDE.md`: `Never` zone files — see Section E).
3. Every feature has a test (`AGENTS.md` Section G).
4. Writer sees control (`accept/modify/skip/stop`) visibly — never hidden (`AGENTS.md` Section D).
5. Writer feels empowered, not replaced (`USER_GUIDE.md` philosophy).

<!-- END PROTECTED -->

---

## B. PROJECT STATE (What Exists, What's Frozen, What's Open)

| Component | Path | State | Edit Rule |
|---|---|---|---|
| Engine (frozen) | `crates/inference/src/`, `engine/src/`, `grammar/src/`, `bnf-engine/src/lib.rs` | Done & tested | Never touch (see E.1). Only fix bugs blocking frontend. |
| Story engine | `crates/agent/src/story_engine.rs`, `mecha_agent.rs` | Done (`story_engine.rs` ~954 lines; `mecha_agent.rs` ~990 lines) | Caution zone: read header markers before editing (`AGENTS.md` Section H.3). |
| Desktop widgets | `crates/ui/src/*.rs` | Partial: `pacing.rs`, `markdown_editor.rs` (~1230 lines), `chat.rs`, browsers exist; no visible standalone tests yet | Always edit: add `#[cfg(test)]` module before wiring (`TASK_01_DESKTOP_WIDGETS.md`). |
| CLI entry | `start.sh` | Works | Always edit freely. |
| CLI binary | `crates/cli/src/bin/roco.rs` (~1373 lines) | Works | Always edit: header has section map. |
| Web apps | `apps/chat/`, `studio/`, `editor/` | Untested; deprecated for new features (`STRATEGIC_PLAN.md` Phase 4) | Edit only for bug fixes; new features go to `crates/ui/`. |
| Missing docs (fixed) | `COMMANDS.md`, `EDITOR.md`, `PLUGINS.md`, `API.md` | Created 2026-07-20 | Edit freely to expand if APIs change. |
| Agent guide | `AGENT_GUIDE.md` | Short rules (exists) | Always edit freely. |
| Edit boundaries | `EDIT_GUIDE.md` | Full zones (exists) | Always edit freely. |
| User guide | `USER_GUIDE.md` | End-user journey (exists) | Always edit freely. |

**Current phase:** Experience-first (`STRATEGIC_PLAN.md` Section A.4). Active: desktop widget standalone-first build (`Phase 2` in strategic plan).

---

## C. KEY COMMANDS (Always Use These — Never Invent New Ones)

```bash
# Verification (run before any edit commit)
run_tests.sh              # cargo check + clippy + test --no-run + notes

# Quick desktop check
run_desktop.sh

# Quick CLI start (user-facing)
./start.sh
./start.sh "premise text"

# Workspace tests
cargo test --workspace --no-run
cargo clippy --workspace --all-targets -- --deny warnings

# Story pipeline (canonical user surfaces)
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli
RWKV_MODEL=... cargo run --release --example story_collaborative -p roco-cli

# Subcommands (see COMMANDS.md for full reference)
roco eval [--output PATH]     # Eval + snapshot
roco bless [--snapshot PATH]  # Bless oracles
roco rwkv                      # Backend smoke test
roco grammar                   # Grammar smoke test
roco gpu-check [--json]        # GPU + model info
roco server [--detach]         # HTTP server (for plugins/web)
roco gui                       # Desktop GUI (starts gateway + backend)
roco interact [--interactive] [--resume SESSION] [--pace MODE]
```

**If a command fails:** See `STRATEGIC_PLAN.md` Phase 2 troubleshooting (`TASK_01_DESKTOP_WIDGETS.md` Section `If this fails, do this`).

---

## D. ARCHITECTURE (Simplified — Read Full `PROJECT_STRUCTURE.md` For Deep Dive)

```
User Input (premise / feedback / command)
  ↓ CLI: start.sh / interact  OR  Desktop: desktop_app.rs
  ↓
AppContext (crates/app/src/lib.rs) — single surface primitive
  ↓
StoryEngine (crates/agent/src/story_engine.rs) — outline → plot → chapter
  ↓ MechanisticAgent (crates/agent/src/mecha_agent.rs) — plan-first loop
  ↓ Grammar (crates/grammar/src/bnf.rs + kbnf isolation in bnf-engine)
RwkvBackend (crates/inference/src/backend.rs) — actor thread, BNF-constrained
  ↓
Workspace (crates/workspace/src/workspace.rs) — sandbox .roco/workspaces/
```

**Two agent patterns (`AGENTS.md` original Section, confirmed in `mecha_agent.rs`):**
- **Plan-first (deterministic):** `classify()` → `Intent` (`INTENT_GRAMMAR`) → `derive()` → `Plan` (`PLAN_GRAMMAR`) → `dispatch()` (classic Rust loop) → `commit()` (workspace snapshot). Used for structured story pipeline.
- **ReAct (open-ended):** `Agent::run()` — model-driven loop; model emits `final_answer`. Used for exploratory chat (`agent_chat.rs`, `chat.rs` example).

**Key constraint (`AGENTS.md` original, experimentally validated `prompt_probe_eval.rs`):** Every LLM call uses BNF grammar. Free-form prompting on RWKV-7 2.9B produces `<think>` contamination. Suppress via `NO_THINK_PREFILL` (`<think></think>` prefill in `engine/src/backend.rs`) for content stages; allow `<think>` only in reasoning stages (outline, plot-state, quality) and strip before parsing JSON.

---

<!-- BEGIN PROTECTED -->

## E. BOUNDARIES (Always / Ask First / Never — Exact File Lists)

Read `EDIT_GUIDE.md` before any edit. File header markers confirm zone (`FILE STATUS:` at top of `mecha_agent.rs`, `story_engine.rs`, `desktop_app.rs`, `roco.rs`).

### E.1 Never (Edit Only If Blocking Feature — Minimal Change Only)

- `crates/inference/src/backend.rs`, `actor.rs`, `sampling.rs`, `quant.rs`
- `crates/engine/src/backend.rs`, `eval.rs`, `cases.rs`
- `crates/grammar/src/*.rs` (API changes break implementors in `agent/`, `message/`)
- `crates/bnf-engine/src/lib.rs` (isolated `kbnf`; any edit = `E0275` risk)
- `crates/session/src/*.rs`, `message/src/*.rs`, `tools/src/*.rs`, `workspace/src/*.rs`
- `vendor/web-rwkv/` (patch via `Cargo.toml` only)
- `.envrc`, `devenv.nix`, `flake.nix`

**If you think you need to edit a frozen file:** Check `EDIT_GUIDE.md` Section `Caution Zone`. Confirm the bug actually blocks a frontend feature (`desktop_app.rs`, `start.sh`, `cli/examples/`). Document reason in commit. Keep change to <10 lines if possible.

### E.2 Always (Edit Freely — Add Tests)

- `start.sh`, `run_desktop.sh`, `run_tests.sh`
- `crates/cli/src/bin/roco.rs`, `interact.rs`
- `crates/cli/examples/*.rs` (`story_human.rs` = canonical user surface)
- `crates/ui/src/*.rs` (desktop widgets — standalone-first: `#[cfg(test)]` before `desktop_app.rs` wiring)
- `crates/app/src/lib.rs`, `context.rs`, `daemon.rs`, `workspace.rs` (caution zone: test with `cargo test -p roco-app`)
- `crates/agent/src/interaction.rs`, `natural_feedback.rs`, `outline_editing.rs`, `commentary.rs`, `chapter_steering.rs`, `quality.rs`
- `apps/*` (bug fixes only — no new features; see `STRATEGIC_PLAN.md` Phase 4)
- `docs/`, guides (`README.md`, etc.), `roadmap/`
- `AGENTS.md` (update through `AGENTS.md` Section K)

### E.3 Ask First (Caution Zone — Read Before Edit)

- `crates/app/src/lib.rs` (re-exports used by cli, ui, server; missing export breaks surfaces)
- `crates/app/src/context.rs` (core primitive; `AppContext` creation)
- `crates/app/src/daemon.rs` (background daemon)
- `crates/agent/src/lib.rs` (25 module re-exports)
- `crates/agent/src/mecha_agent.rs` (hidden links: `plan.rs`, `context.rs`, `scheduler.rs`)
- `crates/agent/src/common_agent.rs` (ReAct loop; links to `mecha_agent.rs`)
- `crates/agent/src/story_engine.rs` (core pipeline; method renames break examples)

**If editing a caution-zone file:** Read file header markers first (`FILE STATUS:` shows zone). Check `EDIT_GUIDE.md`. Run full workspace tests (`run_tests.sh`) before committing. If any crate breaks, revert or fix immediately.

<!-- END PROTECTED -->

---

## F. TECH STACK & VERSIONS (Keep Accurate — Update In `Roadmap/Progress.md`)

| Layer | Technology | Version / Note | File Reference |
|---|---|---|---|
| Workspace | Rust | Resolver 2, edition 2021 (`Cargo.toml`) | `Cargo.toml` |
| Language | Rust | 1.70+ (`devenv.yaml`) | `devenv.yaml` |
| Inference | RWKV-7 2.9B (`models/*.st`) | Auto-detected; `models/` gitignored (`.gitignore`) | `README.md` Env vars |
| Backend | `RwkvBackend` | `inference/src/backend.rs` | `crates/inference/src/backend.rs` |
| Grammar | `kbnf` 0.5 (`bnf-engine/`) | `Cargo.toml` patch (`vendor/web-rwkv/`) | `Cargo.toml` `[patch.crates-io]` |
| Desktop | `egui` + `eframe` (`crates/ui/`) | Chosen 2026-07-19; `gpui` rejected (`roadmap/blocked.md`) | `roadmap/blocked.md` |
| CLI | `clap` 4 (`crates/cli/`) | `roco` binary + examples | `crates/cli/src/bin/roco.rs` |
| Web apps | `Next.js` (`chat/`), `Vite` (`editor/`) | Untested; migration target `crates/ui/` (`STRATEGIC_PLAN.md`) | `apps/chat/package.json` |
| Session store | `LruSessionPool` | Max 8 (`session/src/pool.rs`) | `crates/session/src/pool.rs` |
| Workspace | `Workspace` (`workspace/src/`) | Sandbox `.roco/workspaces/` | `crates/workspace/src/workspace.rs` |
| Testing | `cargo test --workspace` | No redirect (`>`); read terminal directly (`AGENTS.md` original rules) | `run_tests.sh` |

---

## G. TESTING STRATEGY (Every Feature Includes A Test)

From `AGENTS.md` original (`roadmap/README.md` Definition of Done): A feature is not done without a test proving a human can drive it.

**Unit tests:** Mode conversion (`pacing.rs`), plot-state merge (`story_engine.rs` tests), grammar parsing (`grammar/src/grammar_library.rs` tests).
**Integration tests:** CLI output (`start.sh` produces readable workspace files), desktop widget standalone (`pacing::tests`), desktop end-to-end (`tests/desktop_e2e.rs` — target from `STRATEGIC_PLAN.md` Phase 3.5).
**Snapshot/bless:** `roco eval` saves `.snapshot.json`; `roco bless` updates `oracle:` fields (`crates/cli/src/bin/roco.rs` `cmd_bless()`).
**No hidden failures:** Never redirect (`>` `2>&1`). If inference hangs in debug, use `RWKV_ADAPTER=llvmpipe` (`AGENTS.md` original Section `Build with --release`).

---

## H. CRITICAL FILE MAP (Navigate Without Reading Everything)

Every large source file has a `FILE STATUS:` header (added 2026-07-20). Read that first.

| Need | File (Header Line) | Section Keys | Do Not Touch (Unless Blocking) |
|---|---|---|---|
| CLI binary wiring | `crates/cli/src/bin/roco.rs` (line 4) | `spawn_detached`, `cmd_server`, `cmd_story`, `cmd_interact` (lines 15-120, 120-500, 700-1370) | `vendor/web-rwkv/` |
| Mechanistic agent | `crates/agent/src/mecha_agent.rs` (line 4) | `RepairConfig`, `MechanisticAgent::new()`, `register`, `dispatch_single`, `run`, `BaseAgent` impl, `INTENT_GRAMMAR`, `PLAN_GRAMMAR`, `tests` (lines 4, 50-120, 150-300, 400-600, 700-950, 950-990) | `crates/bnf-engine/src/lib.rs` |
| Story pipeline | `crates/agent/src/story_engine.rs` (line 4) | `PlotState`, `OutlineExpansion`, `StoryConfig`, `StoryEngine`, `generate_outline`, `expand_outline`, `generate_chapter`, `evaluate_chapter_quality`, `revise_chapter`, `publish` (lines 4, 30-200, 250-650, 650-950, 950+) | `crates/inference/src/actor.rs` |
| Desktop app | `crates/ui/src/desktop_app.rs` (line 4) | `RightPanelTool`, `RocoDesktopApp`, `new()`, `handle_chat_action`, `show_right_panel`, `update()` (lines 12-45, 47-115, 200-450, 450-600, 600-900) | — (editable; standalone-first) |
| Core surface primitive | `crates/app/src/lib.rs` (line 4) | `AppContext`, `AppError`, `block_on`, `generate` | — (caution zone; test `-p roco-app`) |
| Grammar library | `crates/grammar/src/grammar_library.rs` | `StoryGrammar` (embedded `GBNF/` files) | `crates/grammar/src/bnf.rs` (core) |
| Project structure | `PROJECT_STRUCTURE.md` | Three "app" naming explanation (`crates/app/`, `crates/ui/`, `apps/`) | — |
| User journey | `USER_GUIDE.md` | CLI, web editor, desktop GUI, giving feedback, resuming, common questions | — |
| Edit rules | `EDIT_GUIDE.md` | Frozen / Editable / Caution zones; `Never Edit These`; quick workflow (`cat` → edit → `run_tests.sh`) | — |
| Agent behavior | `AGENTS.md` (this file) | All sections; protection markers; maintenance rules | Sections between `<!-- BEGIN PROTECTED -->` / `<!-- END PROTECTED -->` |

---

<!-- BEGIN PROTECTED -->

## I. COMMON PITFALLS (`Symptom → Cause → Fix`)

Based on `AGENTS.md` original `Lessons Learned` and verified experimental results (`prompt_probe_eval.rs`, `token0_probe.rs`):

| Symptom | Cause | Fix |
|---|---|---|
| `<think>` tags leak into story output | Bare `Assistant:` start opens `<think>` block; system "no think" instruction backfires (primes emission) (`prompt_probe_eval.rs` verified) | Prefill `NO_THINK_PREFILL` (`<think></think>`) before assistant turns (`engine/src/backend.rs`). Allow `<think>` only in reasoning stages (outline, plot-state, quality) and strip before parse. |
| Agent tries to edit frozen engine file | Unclear file boundary; missing header marker check | Read `FILE STATUS:` at top of file (`mecha_agent.rs`, `story_engine.rs`, etc.). Check `EDIT_GUIDE.md`. Confirm bug blocks frontend feature. |
| Desktop freezes on chat/generate | `.await` called inside `update()` loop (`desktop_app.rs`) | Always use `futures::executor::block_on()` for backend calls in GUI (`desktop_app.rs` lines handling chat actions). |
| Large file split incorrectly | No section markers; agent guesses split points | All large files now have header markers (`FILE STATUS:` at line 4 of `roco.rs`, `mecha_agent.rs`, `story_engine.rs`, `desktop_app.rs`). Read markers first. |
| Agent edits untested web apps extensively | `apps/` treated as primary surface; no clear deprecation note | `README.md` and `PROJECT_STRUCTURE.md` clearly state `crates/ui/` is planned primary; `STRATEGIC_PLAN.md` Phase 4 freezes web apps. Read strategic plan before `apps/` edits. |
| Test output hidden / redirected | Agent uses `>` or `2>&1` out of habit (`AGENTS.md` original `Testing convention`) | Never redirect. Fix failure instead of hiding it (`run_tests.sh` notes this explicitly). |
| Desktop widget fails after wiring | Widget not tested standalone before composition (`roadmap/ux.md` `standalone-first` rule) | Build widget test (`#[cfg(test)]`) before editing `desktop_app.rs`. See `TASK_01_DESKTOP_WIDGETS.md` Phase 2.1-2.4. |

<!-- END PROTECTED -->

---

## J. RESEARCH SYNTHESIS (What Works — No Independent Research Needed)

This section exists so the agent does not need to research. Everything below is synthesized from verified sources and cited by reference to repository files or external studies.

### J.1 Agent-Editable Repository Patterns (From ETH Zurich + Production Repos)
- **Manual > auto (`AGENTS.md` v2.0 intro, ETH Zurich study reference).**
- **Progressive disclosure (`EDIT_GUIDE.md` links to deeper docs; `PROJECT_STRUCTURE.md` for navigation).**
- **Hierarchical precedence (`AGENTS.md` root + file-level markers; nearest file wins for subpackages — this repo uses single root + header markers rather than 88 nested files, appropriate for ~19 crates).**
- **Always include:** Agent Role (`A`), Tech Stack (`F`), Commands (`C`, with exact flags), Architecture (`D`), Boundaries (`E`), Critical Files (`H`), Pitfalls (`I`), Maintenance (`K`).

### J.2 Collaborative Writing UX (From Multi-Agent Studies)
- **Writer-Editor loop (`story_engine.rs`: generate → evaluate → revise) improves quality (`arXiv 2605.29625`).**
- **Direct manipulation (`desktop_app.rs`: `LinkGraph`, `ChangeTimeline`) fosters playful collaboration (`PlayWrite` 2018, `StoryEnsemble` 2025).**
- **Pace control must be visible (`PacingWidget` maps to `InteractionMode` — `AGENTS.md` Section A.4, `pacing.rs`).**

### J.3 Desktop (`egui`) Architecture (From `egui` Core + Production Apps)
- **State-first (`desktop_app.rs`: `RocoDesktopApp` owns all widget states).**
- **Standalone-first (`STRATEGIC_PLAN.md` Phase 2.1-2.4; `TASK_01_DESKTOP_WIDGETS.md`).**
- **No `.await` in `update()` (`desktop_app.rs` uses `block_on`).**

---

## K. MAINTENANCE RULES (How This File Stays Accurate)

Based on `AGENTS.md` Guidelines (GitHub Gist) and `AGENTS.md` v2.0 design:

| Trigger | Action | Where To Record |
|---|---|---|
| Agent made preventable mistake | Add boundary (`Never` / `Ask First`) | Section E + `EDIT_GUIDE.md` |
| New widget/pattern established | Add to `Good Patterns` (`EDIT_GUIDE.md`) + reference in `Critical File Map` (Section H) | `EDIT_GUIDE.md` |
| Dependency/version changed | Update `Tech Stack` (Section F) | Section F + `Cargo.toml` |
| New subcommand added (`roco`) | Add to `Key Commands` (Section C) with exact flags | Section C |
| Feature done (tested) | Append line to `roadmap/progress.md` | `roadmap/progress.md` (append-only) |
| Quarterly review | Remove stale file paths, verify commands compile, check protection markers intact | This file (`AGENTS.md`) |
| After any edit to `AGENTS.md` | Note version/date at top of file (`> **Version:** X.Y | **Date:** YYYY-MM-DD`) | Top of file |

**Protection rule:** Sections between `<!-- BEGIN PROTECTED -->` and `<!-- END PROTECTED -->` can only be edited by the human. If an agent needs to propose changes to protected sections, it must ask the user explicitly (per Section E.3 `Ask First` protocol: confirm blocking feature, document reason, propose minimal change, ask confirmation).

**File size target:** Root `AGENTS.md` should stay under ~200 lines (`AGENTS.md` research: shorter = better agent performance). If it grows beyond this, split into nested `AGENTS.md` files (`crates/ui/AGENTS.md` for desktop-specific rules, `crates/cli/AGENTS.md` for CLI-specific). For this repo (19 crates, one primary desktop surface), a single root file with progressive disclosure (`EDIT_GUIDE.md`, `TASK_*.md` files) is sufficient.

---

*This file is version-controlled. Every edit requires a dated entry at the top (`Version` line) and an update to `roadmap/progress.md`. See `AGENTS.md` Section K for full rules.*
