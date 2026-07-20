# TASK 10 — CONTINUOUS MAINTENANCE SYSTEM (Agent + Project Health)

> **Reference:** `AGENTS.md` Section K (`Maintenance Rules`); `AGENTS.md` Section A (`Agent Role` priority 1: build stays green); `AGENTS.md` Section I (`Pitfalls` — symptom-cause-fix); `EDIT_GUIDE.md`; `roadmap/progress.md` (append-only progress tracking); `AGENTS.md` top (`Version` line tracking).
> **Status:** Ongoing — never "done." This task creates the system that keeps `AGENTS.md`, `EDIT_GUIDE.md`, `TASK_*.md`, and source markers accurate over time.
> **Why:** `AGENTS.md` Guidelines (GitHub Gist, `AGENTS.md` research): files drift; stale patterns mislead agents; quarterly review + incident updates prevent decay.

---

## PROBLEM (Why Maintenance Fails Without System)

Without a structured maintenance process:
- `AGENTS.md` grows stale (file paths change, new patterns emerge, frozen engine rules become outdated if bugs are fixed).
- `EDIT_GUIDE.md` boundaries become incorrect (new editable files added, frozen files modified with minimal fixes — no record of why).
- Source file header markers (`FILE STATUS:`) become incorrect if file status changes (e.g., `desktop_app.rs` edited extensively — still `Always`? `mecha_agent.rs` edited for bug fix — still `Caution`? Need documentation).
- `TASK_*.md` files accumulate; completed phases don't get archived or updated.
- `roadmap/progress.md` not updated; trajectory invisible.
- Agent makes preventable mistake; missing boundary added to `AGENTS.md` Section E? Not recorded.

---

## MAINTENANCE SYSTEM DESIGN (Based On `AGENTS.md` Section K + Research)

### K.1 Version Tracking (`AGENTS.md` Top Line)

Every edit to `AGENTS.md` updates top line:
```
> **Version:** X.Y | **Date:** YYYY-MM-DD | **Status:** [Updated / Verified / No changes needed]
```

**Check command:**
```bash
echo "> **Version:** $(grep -m 1 'Version:' AGENTS.md | sed 's/.*Version: //' | cut -d' ' -f1) | **Date:** $(date +%Y-%m-%d)"
```
**If version line doesn't exist or date is old:** Update it. This is non-optional for every edit.

---

### K.2 Quarterly Review Checklist (Every 3 Months — Use `TASK_10_QUARTERLY_CHECK.md` or Manual Check)

Run this checklist manually or document results in `TASK_10_MAINTENANCE.md` (this file, bottom section `Quarterly Results`):

```
QUARTERLY REVIEW CHECKLIST (AGENTS.md Maintenance)

[ ] 1. TECH STACK ACCURACY
    Command: grep -n "version\|Version\|Rust\|egui" AGENTS.md
    Verify: Versions match Cargo.toml, devenv.yaml, README.md.
    Fix: Update Section F if mismatched.

[ ] 2. BOUNDARY ACCURACY
    Command: cat EDIT_GUIDE.md | grep -A 2 "Never\|Always\|Ask First"
    Verify: Frozen files haven't been edited incorrectly (check git log for engine files).
    Verify: Editable files have tests (TASK_01-09 milestones reference tests).
    Fix: Update EDIT_GUIDE.md if new caution zone identified; add incident note to Section E.

[ ] 3. CRITICAL FILE MAP ACCURACY
    Command: grep -n "FILE STATUS" crates/cli/src/bin/roco.rs crates/agent/src/mecha_agent.rs crates/agent/src/story_engine.rs crates/ui/src/desktop_app.rs crates/app/src/lib.rs
    Verify: All large files still have header markers; status matches EDIT_GUIDE.md.
    Fix: Add/revise markers if file edited extensively.

[ ] 4. TEST COVERAGE (Desktop Widgets)
    Command: cargo test -p roco-ui --no-run 2>&1 | tail -n 5
    Verify: All widget standalone tests compile; desktop_e2e test exists (TASK_03).
    Fix: If new widget added without test, add `#[cfg(test)]` (TASK_01 procedure).

[ ] 5. ROADMAP PROGRESS
    Command: tail -n 10 roadmap/progress.md
    Verify: Last entry within 3 months; dated; includes what/where/done status.
    Fix: Append review summary line.

[ ] 6. COMMON PITFALLS STALENESS
    Command: grep -n "symptom\|cause\|fix" AGENTS.md | head -n 10
    Verify: Pitfalls reference real issues (check recent agent mistakes, commit messages, issue notes).
    Fix: Add new pitfall if agent made preventable error; update fix reference.

[ ] 7. RESEARCH SYNTHESIS ACCURACY
    Command: grep -n "ETH Zurich\|StoryEnsemble\|PlayWrite\|egui core" AGENTS.md
    Verify: Citations still accurate; no contradictory new evidence (search for updates if needed).
    Fix: Update research references if new studies published; document changes.

[ ] 8. PROTECTION MARKERS INTACT
    Command: grep -c "BEGIN PROTECTED" AGENTS.md; grep -c "END PROTECTED" AGENTS.md
    Verify: Counts match (should be equal pairs); protected sections unedited by agent.
    Fix: If agent edited protected section, revert; document why protection needed.
```

---

### K.3 Incident Response Protocol (When Agent Makes Preventable Mistake)

**Step 1 — Stop.** Don't continue with same pattern.
**Step 2 — Identify missing boundary.** Read `EDIT_GUIDE.md` zones. Check if file edited was `Never` or `Ask First` without confirmation.
**Step 3 — Add boundary to `AGENTS.md` Section E.** Add new `Never` entry or expand `Ask First` note with exact file path.
**Step 4 — Add common pitfall to `AGENTS.md` Section I.** Format: `| Symptom | Cause | Fix |`.
**Step 5 — Verify fix.** Re-run `run_tests.sh`. Confirm build green.
**Step 6 — Record.** Append to `roadmap/progress.md`: `YYYY-MM-DD | Incident: [brief] | Boundary added: [file] | Pitfall added: Section I | Status: Fixed`.
**Step 7 — Update version.** `AGENTS.md` top line: `Version: X.Y+1 | Date: YYYY-MM-DD | Status: Updated — incident response`.

---

### K.4 Feature Completion Tracking (Link `TASK_*.md` To `Roadmap/Progress.md`)

When any `TASK_01` through `TASK_09` milestone completes:

```bash
# Append to roadmap/progress.md
echo "YYYY-MM-DD | TASK_XX_PHASE_X completed: [milestone description] | File: TASK_XX_*.md | Status: Done" >> roadmap/progress.md
```

**Example entry:**
```
2026-07-20 | TASK_01_DESKTOP_WIDGETS Phase 2.1: PacingWidget standalone test passes | File: crates/ui/src/pacing.rs | Status: Done
2026-07-22 | TASK_02_DESKTOP_INTERACTION Phase 3.2: Interactive mode wired (Pacing → StoryEngine) | File: desktop_app.rs + interaction.rs | Status: Done
```

---

## MAINTENANCE CHECKLIST (Concrete Commands — Use Monthly Or After Any Major Change)

```bash
# 1. Verify all task files exist and reference correct milestone status
ls TASK_01_DESKTOP_WIDGETS.md TASK_02_DESKTOP_INTERACTION.md TASK_03_DESKTOP_E2E.md TASK_04_EDITOR_MIGRATION.md TASK_05_CHAT_MIGRATION.md TASK_06_STUDIO_INTEGRATION.md TASK_07_WEB_FREEZE.md TASK_08_PLUGIN_VERIFY.md TASK_09_API_UPDATE.md TASK_10_CONTINUOUS_IMPROVEMENT.md

# 2. Verify AGENTS.md version line updated
head -n 3 AGENTS.md

# 3. Verify protected markers intact
python3 -c "content=open('AGENTS.md').read(); print('PROTECTED pairs:', content.count('BEGIN PROTECTED') == content.count('END PROTECTED'))"

# 4. Verify edit boundaries match current code
cat EDIT_GUIDE.md | grep -c "crates/ui/src/"

# 5. Verify quick tests green
run_tests.sh

# 6. Verify desktop launches
run_desktop.sh 2>&1 | head -n 5  # Just confirm starts; don't hang

# 7. Verify documentation consistent
# (Manual: compare README.md Three Surfaces to desktop_app.rs panel list; compare PLUGINS.md to apps/plugins/)
```

---

## IF MAINTENANCE FAILS (What To Do)

| Check Fails | Immediate Action | Reference |
|---|---|---|
| `PROTECTED` pairs don't match | Read `AGENTS.md`; find edited protected section; revert edit; document incident; add boundary if missing. | `AGENTS.md` Section E (protected sections) |
| `EDIT_GUIDE.md` references outdated file paths | Check `PROJECT_STRUCTURE.md` directory map; compare to actual `find crates/ apps/ docs/`; update references. | `PROJECT_STRUCTURE.md` |
| `run_tests.sh` fails | Read error message directly (`AGENTS.md` Section G); fix failure; don't redirect/hide. Check `TASK_01` milestone status — if new widget added without test, add `#[cfg(test)]` per Phase 2 procedure. | `AGENTS.md` Section G |
| `run_desktop.sh` crashes | Read crash message; check `desktop_app.rs` line referenced. Confirm `Always` zone edit didn't break widget (`TASK_01` prerequisite). Revert `desktop_app.rs` change if needed; fix widget standalone first. | `TASK_01_DESKTOP_WIDGETS.md` troubleshooting |
| `TASK_XX` milestone outdated (done but file says not done) | Update `TASK_XX_*.md` milestone line with `Done` and date; append to `roadmap/progress.md`. | `TASK_XX_*.md` milestone section |
| `TASK_XX` milestone says done but feature broken | Re-open milestone (change to `Not done`); document failure reason; fix; confirm fix; update milestone. Don't leave false `Done` status. | `AGENTS.md` Section 7 (`Be honest about partial work`) |
| `AGENTS.md` exceeds ~250 lines (performance risk) | Split into nested file (`crates/ui/AGENTS.md` for desktop rules, `crates/cli/AGENTS.md` for CLI rules). Root `AGENTS.md` keeps universal rules; subdirectory files add local deltas (`AGENTS.md` Guidelines — hierarchical precedence). Update `PROJECT_STRUCTURE.md` to reference nested files. | `AGENTS.md` Section K.4 |

---

*This maintenance system references `AGENTS.md` Sections A, E, G, I, K; `EDIT_GUIDE.md`; `PROJECT_STRUCTURE.md`; `TASK_01` through `TASK_09`; `STRATEGIC_PLAN.md`; `roadmap/progress.md`. It is designed to run continuously — no "completion" milestone exists for maintenance. Every edit to `AGENTS.md` requires version update (`K.1`); every agent mistake requires incident protocol (`K.3`); every quarter requires checklist (`K.2`).*
