# Jules Session Archive

Operational record of Jules API sessions that are **unusable** — FAILED, or
superseded duplicates whose work already landed in the repo. They cannot be
deleted via the v1alpha API (no cancel/delete endpoint exists), so they are
closed out here. Read-only state; do not revive these sessions.

Archived: 2026-08-02. See `scripts/jules.sh` for live session management.

---

## FAILED sessions (6)

| Session ID | Title | Created | Last update | Last activity |
|---|---|---|---|---|
| `14120584308420221258` | Integrated arXiv and Camoufox Research System | 2026-08-01 10:11 | 2026-08-01 10:40 | Implemented Camoufox Web Scraping System in Rust; failed before completion |
| `8620469840729202413` | Add flaky test detection and prevention (workspace-wide non-determinism review) | 2026-08-01 13:39 | 2026-08-01 14:18 | Failed mid-investigation |
| `15594329506628884314` | Implement story branching and merge capability | 2026-08-02 06:30 | 2026-08-02 06:43 | Failed (duplicate of landed `roco story branch`, commit `119333d`) |
| `13465324747186126277` | UX improvements §12 (quickstart/spinners/errors) | 2026-08-02 08:05 | 2026-08-02 08:29 | Produced a full indicatif-spinner diff for `story.rs` but failed — patch was buggy (double `pb.finish_and_clear()`, `spinner_style` scoping) |
| `15251794233235826841` | Implement story branching and merge capability | 2026-08-02 06:30 | 2026-08-02 08:32 | "Executed tests which passed, clippy passes locally" then failed; duplicate of landed work |
| `3969969234300664741` | UX improvements §12 | 2026-08-02 08:05 | 2026-08-02 08:39 | Failed; duplicate of the UX task (owned by `10186746275937479489`) |

## Superseded hanging sessions (7) — reply sent: "already merged, stop"

These were `AWAITING_USER_FEEDBACK`; their task has since landed in the repo
(verified in git/AGENTS.md). Each received a stop message on 2026-08-02.

| Session ID | Title | Stuck since | Superseded by |
|---|---|---|---|
| `16049664970031308780` | Improve CLI error messages with actionable hints | 2026-08-01 13:52 | Hints landed in `crates/gateway/src/lib.rs` (applied `update_errors.patch`) |
| `16612255580972289195` | Add `show_work` management intent to router | 2026-08-01 14:01 | Landed — AGENTS.md §13 (keyword router + `show_work`) |
| `10121677104426494750` | Add `new_project` management intent to router | 2026-08-01 14:03 | Landed — AGENTS.md §13 (`new_project` keyword intent) |
| `16445039062750635257` | WFC test flaky: `test_cli_wfc_map_generation` ROCO_DIR race | 2026-08-01 18:28 | Fixed — PROGRESS.md 2026-08-01 (harness `with_env`, 3× green runs) |
| `17981227535420543824` | Add `--preview` flag to `roco story` | 2026-08-01 19:53 | Landed — completed session `8476536743818790608` (2026-08-01 20:34) |
| `12412840637205810569` | Implement session persistence tests | 2026-08-01 20:32 | Landed — completed session `15216642766507773818` (2026-08-01 13:47) |
| `17289339326306258117` | Story branching and merge capability | 2026-08-02 06:40 | Landed — commit `119333d` `roco story branch` |

## Task owners (kept after dedup)

Sessions told to **proceed and open a PR** (one per task, to avoid conflicting PRs):

| Session ID | Task | Outcome |
|---|---|---|
| `10186746275937479489` | UX improvements §12 | completed **without pushing** — claimed done, but quickstart/spinners never landed (still open §12 items) |
| `1790401646888824729` | `roco story continue` | completed without pushing — **changeSet extracted and landed** in `67ef06b` |
| `7057193458828796296` | collaborative revisions (`revise`) | completed without pushing — **changeSet extracted and landed** in `67ef06b` |
| `1704905722071487336` | auto-managed session/workspace state | completed without pushing — **changeSet extracted; help-hidden part landed**, auto-resume hunk rejected (stale signature + wrong semantics) |
| `16415932402413650959` | The Warden's Folio (creative writing) | completed, 2 outputs delivered |

Duplicate sessions told to **stop** (15): `7675478194000220051`, `11357377229008668739`,
`3521386786704750735`, `18396354018188246689`, `2633546216780420868`,
`5362387017989136640`, `225514095709198024`, `17339046685329745878`,
`14387480738433465795`, plus the 7 superseded sessions above.

## Key reference

- Jules API: https://developers.google.com/jules/api
- Live management: `scripts/jules.sh` (`check | sources | sessions | session | activities | send | create | approve | curl`)
- No delete/cancel endpoint exists in v1alpha (probed `:cancel` → 404) — archived sessions remain in the Jules web UI history.
