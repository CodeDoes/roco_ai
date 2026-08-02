# Jules Session Archive

Complete index of **all Jules (Bolt) API sessions** for the `roco_ai` repo — every task,
its outcome, and where its work landed (if it did). Sessions cannot be deleted via the
v1alpha API (no cancel/delete endpoint — probed `:cancel` → 404), so this file is the
permanent closure record. **Read-only: do not revive archived sessions.**

Live management: `scripts/jules.sh` (`check | sources | sessions | session | activities | send | create | approve | curl`).

Archived (complete): 2026-08-02.

---

## Summary

- **125 sessions total**: 95 for `roco_ai`, 30 for other repos.

- Session states: 89 completed / 6 failed (roco_ai only).

- **Recurring failure mode**: agents complete tasks but never push a branch/PR — the work

  exists only as `unidiffPatch` changeSets inside session activities. Use `scripts/jules.sh`

  `activities <id>` + the changeSet to recover it. Several were recovered manually (below).


### ✅ Merged as PR (17)

| Date | Session ID | Task | Evidence |
|---|---|---|---|
| 2026-07-17 | `13636377265769387313` | Refactoring and Stabilizing the RoCo AI Rust Workspace | `Cargo.lock`, `crates/agent/src/base.rs`, `crates/agent/src/mecha_agent.rs` |
| 2026-07-17 | `8360147831957112935` | Comprehensive App Enhancement Suite | `Cargo.lock`, `crates/agent/Cargo.toml`, `crates/agent/src/common_agent.rs` |
| 2026-07-23 | `10052844614878231535` | Codebase Review Suggestions | `.github/workflows/ci.yml`, `Cargo.lock`, `Cargo.toml` |
| 2026-07-23 | `7926610897732011758` | Mocked Local AI Harness and Domain Agent Framework Scaffold | `ACTUAL_CODE_INVENTORY.md`, `EXPANDED_USE_CASES.md`, `FRAMEWORK_EVERYTHING.md` |
| 2026-07-24 | `6796626086556462783` | Refactor Non-DRY Code | `Cargo.lock`, `Cargo.toml`, `crates/agent/src/evals.rs` |
| 2026-07-24 | `10519066996559109175` | Conceptualizing Safer and Simpler Apps | `crates/app/src/local_agent/loop/mod.rs`, `crates/app/src/local_agent/mod.rs`, `crates/app/src/local_agent/sandbox.rs` |
| 2026-07-31 | `6521845017215265445` | Conceptual App Specification and Architecture Blueprint | `docs/TECHNICAL_SPECIFICATION.md` |
| 2026-07-31 | `14695270574116720126` | High-Standard Codebase Overhaul: Story Creation & Editing | `.cargo/config.toml`, `.github/workflows/ci.yml`, `AGENTS.md` |
| 2026-07-31 | `16562552741657937152` | Roco: Chat Implementation, Testing Evals, and Stability Impr | `.cargo/config.toml`, `.github/workflows/ci.yml`, `AGENTS.md` |
| 2026-08-01 | `2305969338615088963` | The Game Master's Multi-Genre Adventure Guidebooks | `docs/ttrpg_guides/adventure.md`, `docs/ttrpg_guides/colony_management.md`, `docs/ttrpg_guides/dating_sim.md` |
| 2026-08-01 | `3071913198606538728` | World Builder & Simulator | `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/mod.rs`, `crates/cli/src/cmd/world_sim.rs` |
| 2026-08-01 | `15714527802185562570` | Wave Function Collapse World Map Creator | `crates/agent/src/embeddings.rs`, `crates/cli/examples/matrix_eval.rs`, `crates/cli/examples/root_bake_test.rs` |
| 2026-08-01 | `16562217018853643142` | Record Asciinema App Demo | `demo.cast` |
| 2026-08-01 | `14891434487895203534` | Practical CI/CD Pipeline Setup | `.github/workflows/ci.yml`, `apps/plugins/vscode/.gitignore`, `apps/plugins/vscode/src/extension.test.ts` |
| 2026-08-01 | `8734906127248152439` | Advanced LLM Inference and Architecture Evaluation Bench | `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/mod.rs`, `crates/cli/src/cmd/solution_bench.rs` |
| 2026-08-01 | `14390178209579120919` | Speed Up Nix CI with Cachix | `.github/workflows/ci.yml`, `crates/cli/evals/results/solution_bench.json` |
| 2026-08-02 | `7029977012429529988` | Bolt ⚡: Performance Optimization Agent | `.jules/bolt.md`, `crates/agent/src/embeddings.rs`, `crates/agent/src/memory.rs` |

### 🟣 Open PR (1)

| Date | Session ID | Task | Evidence |
|---|---|---|---|
| 2026-08-01 | `16415932402413650959` | The Warden's Folio: Five Adventure Tomes for Game Masters | `docs/ttrpg_guides/wardens_folio.html` |

### ⚠️ PR closed-unmerged, feature landed (2)

| Date | Session ID | Task | Evidence |
|---|---|---|---|
| 2026-08-01 | `13640675747004665598` | NLU-Integrated TTRPG and World-Building System | `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/mod.rs`, `crates/cli/src/cmd/ttrpg.rs` |
| 2026-08-01 | `10803353046285119008` | Vector Embedding Search System | `crates/agent/src/embeddings.rs`, `crates/agent/src/lib.rs`, `crates/agent/src/tools/builtins.rs` |

### 📦 Extracted & landed by maintainer (3)

| Date | Session ID | Task | Evidence |
|---|---|---|---|
| 2026-08-01 | `1704905722071487336` | Implement auto-managed session/workspace state. The router s | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/router.rs` |
| 2026-08-02 | `7057193458828796296` | Implement collaborative revisions per crates/cli/tests/futur | `crates/cli/src/cmd/story.rs`, `crates/cli/tests/future_capabilities.rs` |
| 2026-08-02 | `1790401646888824729` | Implement the 'continue' management intent for story session | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/story.rs`, `crates/cli/tests/story_continue_test.rs` |

### ✅ Landed in repo (applied changeSet) (31)

| Date | Session ID | Task | Evidence |
|---|---|---|---|
| 2026-08-01 | `5955481241925233762` | Add a 'roco quickstart' first-run guide command. When a new  | `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/mod.rs`, `crates/cli/src/cmd/quickstart.rs` |
| 2026-08-01 | `11163061934322484857` | Move the keyword-based intent classification from MockBacken | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/router.rs` |
| 2026-08-01 | `178366496838788069` | Add comprehensive tests for grammar-constrained generation.  | `crates/engine/src/grammar/json_schema.rs`, `crates/engine/tests/grammar_tests.rs` |
| 2026-08-01 | `15216642766507773818` | Add comprehensive tests for session persistence. Test file:  | `crates/cli/tests/session_persistence.rs` |
| 2026-08-01 | `17850543371507764771` | Add integration tests for the full story pipeline (outline → | `crates/cli/tests/pipeline_integration.rs`, `crates/engine/src/backend.rs` |
| 2026-08-01 | `13932551167355442593` | Create a test that verifies the keyword-based router works c | `crates/cli/src/cmd/router.rs` |
| 2026-08-01 | `12538986090056139676` | Update AGENTS.md to document the new management intents (sho | `AGENTS.md`, `AGENTS.md.orig` |
| 2026-08-01 | `3428309023724375777` | Add a 'roco status' command that shows: 1) Current mode, 2)  | `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/mod.rs`, `crates/cli/src/cmd/status.rs` |
| 2026-08-01 | `6866988737461598554` | Add unit tests for the router's management intents. Test cas | `crates/cli/src/cmd/router.rs` |
| 2026-08-01 | `17204076462696494780` | Improve error messages in crates/gateway/src/lib.rs to inclu | `crates/gateway/src/lib.rs`, `crates/gateway/src/lib.rs.orig`, `update_errors.patch` |
| 2026-08-01 | `5673509531049320180` | Add timeout handling for long-running operations in the stor | — |
| 2026-08-01 | `15602575536786130945` | Create a quickstart guide in docs/quickstart.md that explain | `docs/quickstart.md` |
| 2026-08-01 | `11686690233943170169` | Create an integration test for the full story pipeline (outl | `crates/cli/tests/story_pipeline_test.rs` |
| 2026-08-01 | `13948550032389326266` | Add timeout handling for long-running story pipeline operati | `crates/agent/src/error.rs`, `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/story.rs` |
| 2026-08-01 | `4763898454694535489` | The WFC test test_cli_wfc_map_generation in crates/cli/tests | `crates/cli/tests/wfc_test.rs` |
| 2026-08-01 | `9893120767658169883` | Add a --version flag to the CLI that prints the version and  | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/status.rs` |
| 2026-08-01 | `14495439255395862146` | Expose the web_rwkv_version in the CLI so users can check th | `crates/cli/build.rs`, `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/bin/roco.rs` |
| 2026-08-01 | `14143853173409814833` | Add retry logic for failed story generation phases in crates | `crates/cli/src/cmd/story.rs` |
| 2026-08-01 | `3902210549425119056` | Add grammar-constrained generation tests | `crates/cli/src/cmd/status.rs`, `crates/engine/src/lib.rs`, `crates/engine/src/tests/bnf_engine_test.rs` |
| 2026-08-01 | `15198892229081921875` | Add CI flake prevention for WFC tests | `crates/cli/src/test_harness.rs`, `crates/cli/tests/wfc_test.rs` |
| 2026-08-01 | `15723845049710152185` | Fix clippy warnings in crates/cli/src/cmd/router.rs - use so | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/status.rs` |
| 2026-08-01 | `10650818835747409303` | Implement parallel CI workflow | `.config/nextest.toml`, `.github/workflows/ci.yml`, `crates/cli/evals/results/solution_bench.json` |
| 2026-08-01 | `8864747656370042463` | Add router NLU keyword classification tests | `crates/cli/src/cmd/router.rs` |
| 2026-08-02 | `7868170523547345349` | Implement the multi-turn narrative editing capability. Look  | `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/story.rs`, `crates/cli/tests/future_capabilities.rs` |
| 2026-08-02 | `10413271644509110586` | Implement the multi-turn narrative editing capability. Look  | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/story.rs`, `crates/cli/tests/future_capabilities.rs` |
| 2026-08-02 | `8898770401631367453` | Implement the multi-turn narrative editing capability. Look  | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/story.rs` |
| 2026-08-02 | `1532844546482160795` | Implement the multi-turn narrative editing capability. Look  | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/bin/roco.rs`, `crates/cli/src/cmd/story.rs` |
| 2026-08-02 | `809913946969590061` | Implement the multi-turn narrative editing capability. Look  | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/story.rs`, `crates/cli/tests/future_capabilities.rs` |
| 2026-08-02 | `10411073022815320977` | Implement story branching and merge capability per crates/cl | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/mod.rs`, `crates/cli/src/cmd/story.rs` |
| 2026-08-02 | `16338343153906111066` | Implement story branching and merge capability per crates/cl | `crates/cli/src/cmd/story.rs`, `crates/cli/tests/future_capabilities.rs` |
| 2026-08-02 | `12606150713688832791` | Implement collaborative revisions per crates/cli/tests/futur | — |

### 🧊 Superseded duplicate — stop sent (16)

| Date | Session ID | Task | Evidence |
|---|---|---|---|
| 2026-08-01 | `16049664970031308780` | Improve CLI error messages with actionable hints. When comma | — |
| 2026-08-01 | `16612255580972289195` | Add a 'show_work' management intent to the router. When a us | — |
| 2026-08-01 | `10121677104426494750` | Add a 'new_project' management intent to the router. When a  | — |
| 2026-08-01 | `16445039062750635257` | The WFC test test_cli_wfc_map_generation was marked as flaky | — |
| 2026-08-01 | `17981227535420543824` | Add a --preview flag to 'roco story' that shows the compiled | — |
| 2026-08-01 | `12412840637205810569` | Implement session persistence tests | — |
| 2026-08-01 | `14387480738433465795` | Implement auto-managed session/workspace state | — |
| 2026-08-02 | `17289339326306258117` | Implement story branching and merge capability per crates/cl | — |
| 2026-08-02 | `17339046685329745878` | Implement collaborative revisions per crates/cli/tests/futur | — |
| 2026-08-02 | `5362387017989136640` | Implement collaborative revisions per crates/cli/tests/futur | — |
| 2026-08-02 | `225514095709198024` | Implement collaborative revisions per crates/cli/tests/futur | — |
| 2026-08-02 | `11357377229008668739` | Implement the 'continue' management intent for story session | — |
| 2026-08-02 | `2633546216780420868` | Implement the 'continue' management intent for story session | — |
| 2026-08-02 | `3521386786704750735` | Implement the 'continue' management intent for story session | — |
| 2026-08-02 | `18396354018188246689` | Implement the 'continue' management intent for story session | — |
| 2026-08-02 | `7675478194000220051` | Implement UX improvements from AGENTS.md §12 that are still  | — |

### 🔬 Completed, no PR — changeSet extractable (18)

| Date | Session ID | Task | Evidence |
|---|---|---|---|
| 2026-08-01 | `904786315020180526` | Add progress spinners during long CLI waits. When the story  | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/story.rs`, `crates/cli/src/rich_output.rs` |
| 2026-08-01 | `43376193927518133` | Add a story preview after publish. After 'roco story' comple | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/story.rs`, `crates/cli/tests/mock_cli_subcommands.rs` |
| 2026-08-01 | `10931321649492676984` | Show the full story output path prominently after publishing | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/story.rs`, `crates/cli/tests/mock_cli_subcommands.rs` |
| 2026-08-01 | `11408372064270425278` | Add a 'continue' management intent to the router. When a use | `crates/cli/src/cmd/router.rs`, `crates/cli/src/cmd/story.rs` |
| 2026-08-01 | `40966944574601100` | Add better error messages when model loading fails. In crate | `crates/engine-gpu/src/config.rs` |
| 2026-08-01 | `12865159206982610980` | Create an integration test for the show_work feature. The te | `crates/cli/src/cmd/router.rs` |
| 2026-08-01 | `9784625986273857388` | Update the CLI help text to include the new management inten | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/lib.rs` |
| 2026-08-01 | `6758725352654304703` | Review the story pipeline in crates/cli/src/cmd/story.rs and | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/story.rs` |
| 2026-08-01 | `2458249735060063710` | Update the README.md to document the new management intents  | `README.md`, `crates/cli/evals/results/solution_bench.json` |
| 2026-08-01 | `8388650021253952921` | Add validation to ensure chapter numbers are sequential in c | `crates/cli/src/cmd/story.rs` |
| 2026-08-01 | `12167752732194728543` | Add progress bars during long CLI waits in crates/cli/src/cm | `Cargo.lock`, `crates/app/Cargo.toml`, `crates/app/src/daemon.rs` |
| 2026-08-01 | `18207128461200737770` | Create docs/management_intents.md explaining the new managem | `crates/cli/evals/results/solution_bench.json`, `docs/management_intents.md` |
| 2026-08-01 | `14755736846997458623` | Review state loading in crates/engine-gpu/src/actor.rs and o | `crates/cli/src/cmd/status.rs`, `crates/engine-gpu/src/actor.rs` |
| 2026-08-01 | `8884421170867051721` | Update help text in crates/cli/src/bin/roco.rs to include ex | `crates/cli/src/lib.rs` |
| 2026-08-01 | `15967067688989217211` | Add error recovery with exponential backoff for failed gener | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/status.rs`, `crates/engine/src/lib.rs` |
| 2026-08-01 | `9768741472268291629` | Add progress indicators during long CLI waits | `Cargo.lock`, `crates/cli/Cargo.toml`, `crates/cli/evals/results/solution_bench.json` |
| 2026-08-01 | `10452549392565097689` | Improve story pipeline performance by reducing allocations | — |
| 2026-08-01 | `8476536743818790608` | Add --preview flag to roco story command to display stories  | `crates/cli/evals/results/solution_bench.json`, `crates/cli/src/cmd/status.rs`, `crates/cli/src/cmd/story.rs` |

### ❌ Failed (6)

| Date | Session ID | Task | Evidence |
|---|---|---|---|
| 2026-08-01 | `14120584308420221258` | Integrated arXiv and Camoufox Research System | — |
| 2026-08-01 | `8620469840729202413` | Add flaky test detection and prevention. Review all tests in | — |
| 2026-08-02 | `15594329506628884314` | Implement story branching and merge capability per crates/cl | — |
| 2026-08-02 | `15251794233235826841` | Implement story branching and merge capability per crates/cl | — |
| 2026-08-02 | `3969969234300664741` | Implement UX improvements from AGENTS.md §12 that are still  | — |
| 2026-08-02 | `13465324747186126277` | Implement UX improvements from AGENTS.md §12 that are still  | — |

### ⚠️ Still-open work — owner completed without landing (1)

| Date | Session ID | Task | Evidence |
|---|---|---|---|
| 2026-08-02 | `10186746275937479489` | UX improvements §12 (quickstart/spinners/errors/preview) | owner session — claimed done, **work NOT landed** (no indicatif, no `--preview`; quickstart *did* land via `8b0fc33` from an earlier session) |

## Non-roco_ai sessions (30) — other repos, not archived here

| Date | Session ID | State | Task | Repo |
|---|---|---|---|---|
| 2026-03-05 | `15104867601527237647` | COMP | Implement velocity matching steering behavior | game-dev task (velocity matching steering) |
| 2026-03-06 | `14294061104615164060` | FAIL | Power Scaling and Documentation Refinement | original_performance_takehome (VLIW kernels) |
| 2026-03-11 | `14058719088113017008` | COMP | Objective:
Connect src/routes/api/generate/+server | Svelte web app (multimodal AI) |
| 2026-03-11 | `5174207530317002379` | COMP | Objective:
Connect src/routes/api/chat/+server.ts  | Svelte web app |
| 2026-03-11 | `4827945186960224277` | COMP | Objective:
Implement the generateMultimodalResult  | Svelte web app |
| 2026-03-11 | `1334094249730695404` | COMP | Objective:
Connect src/routes/api/chat/+server.ts  | Svelte web app |
| 2026-03-11 | `12574769483911978343` | COMP | Objective:
Connect src/routes/api/generate/+server | Svelte web app |
| 2026-03-11 | `1983554067568281348` | COMP | Objective:
Update src/lib/components/GenerationFra | Svelte web app |
| 2026-03-11 | `10097787836228141116` | COMP | Objective:
Fix any failing tests or edge cases dis | Svelte web app |
| 2026-03-11 | `540646293208045456` | COMP | Missing test file for +layout.svelte | Svelte web app |
| 2026-03-11 | `1871961298558032772` | COMP | Commented-out code blocks in App namespace | Svelte web app |
| 2026-03-11 | `16935497792251903279` | COMP | Objective:
Add `aspectRatio` to the genState store | Svelte web app |
| 2026-03-11 | `6804994860212566471` | COMP | Objective:
Polish the Chat panel's text+image outp | Svelte web app |
| 2026-03-11 | `785421470006324310` | COMP | Objective:
Add Playwright E2E tests for clipboard  | Svelte web app |
| 2026-03-18 | `3091545590837256682` | COMP | Remove commented-out App interfaces | Svelte web app |
| 2026-03-18 | `3964751903607889389` | COMP | Missing component test for +layout.svelte | Svelte web app |
| 2026-03-25 | `17686350528415431646` | COMP | Remove placeholder test file | Svelte web app |
| 2026-03-25 | `9430392529171822203` | COMP | Missing Test File for +layout.svelte | Svelte web app |
| 2026-07-17 | `4965955086516320916` | COMP | Self-Directed Theory and Experimentation Project | latent-state-thinking-vs-speaking |
| 2026-07-17 | `15524421758993822951` | COMP | HRM-RWKV-Text Integration | latent-state-thinking-vs-speaking |
| 2026-07-23 | `17883479254757571693` | COMP | BLT-RWKV: Mathematical Validation and Theoretical  | rwkv-lab |
| 2026-07-23 | `1999974213176302052` | COMP | BLT Proofs on Small Model Runs | rwkv-lab |
| 2026-07-23 | `12658744895937403067` | COMP | Port App to Node.js & TypeScript | Svelte web app (Port to Node.js/TS) |
| 2026-07-24 | `13263888723548697095` | COMP | make a RWKV8 toy prototype and train it slightly
 | rwkv-lab |
| 2026-07-31 | `13113377282666784784` | COMP | Optimizing RWKV with Byte Latents | rwkv-lab |
| 2026-08-01 | `3562451623900755702` | COMP | The Game Master's Multi-Genre Adventure Guide Coll | rwkv-lab |
| 2026-08-01 | `16263368156771076855` | COMP | Non-Linear Flow Network with Byte-Latent Tokens an | latent-state-thinking-vs-speaking |
| 2026-08-01 | `1539468230288512396` | COMP | RWKV-KAN with Tiny Stories | rwkv-lab |
| 2026-08-01 | `7045949993432300923` | COMP | Nix Flakes Empire Setup and Package List | flakes |
| 2026-08-01 | `3908728015845795766` | COMP | Training to Achieve Loss Under 0.5 | rwkv-lab-style model training (no repo evidence) |
---

## Failed sessions (6) — detail

| Session ID | Title | Created | Last update | Last activity |
|---|---|---|---|---|
| `14120584308420221258` | Integrated arXiv and Camoufox Research System | 2026-08-01 10:11 | 2026-08-01 10:40 | Implemented Camoufox Web Scraping System in Rust; failed before completion |
| `8620469840729202413` | Add flaky test detection and prevention (workspace-wide non-determinism review) | 2026-08-01 13:39 | 2026-08-01 14:18 | Failed mid-investigation |
| `15594329506628884314` | Implement story branching and merge capability | 2026-08-02 06:30 | 2026-08-02 06:43 | Failed (duplicate of landed `roco story branch`, commit `119333d`) |
| `13465324747186126277` | UX improvements §12 (quickstart/spinners/errors) | 2026-08-02 08:05 | 2026-08-02 08:29 | Produced a full indicatif-spinner diff for `story.rs` but failed — patch was buggy (double `pb.finish_and_clear()`, `spinner_style` scoping) |
| `15251794233235826841` | Implement story branching and merge capability | 2026-08-02 06:30 | 2026-08-02 08:32 | "Executed tests which passed, clippy passes locally" then failed; duplicate of landed work |
| `3969969234300664741` | UX improvements §12 | 2026-08-02 08:05 | 2026-08-02 08:39 | Failed; duplicate of the UX task (owned by `10186746275937479489`) |

## Superseded hanging sessions (7) — detail

These were `AWAITING_USER_FEEDBACK`; their task has since landed in the repo
(verified in git/AGENTS.md). Each received a stop message on 2026-08-02.

| Session ID | Title | Stuck since | Superseded by |
|---|---|---|---|
| `16049664970031308780` | Improve CLI error messages with actionable hints | 2026-08-01 13:52 | Hints landed in `crates/gateway/src/lib.rs` (applied `update_errors.patch`) |
| `16612255580972289195` | Add `show_work` management intent to router | 2026-08-01 14:01 | Landed — AGENTS.md §13 (keyword router + `show_work`) |
| `10121677104426494750` | Add `new_project` management intent to router | 2026-08-01 14:03 | Landed — AGENTS.md §13 (`new_project` keyword intent) |
| `16445039062750635257` | WFC test flaky: `test_cli_wfc_map_generation` ROCO_DIR race | 2026-08-01 18:28 | Fixed — PROGRESS.md 2026-08-01 (harness `with_env`, 3× green runs) |
| `17981227535420543824` | Add `--preview` flag to `roco story` | 2026-08-01 19:53 | Claimed landed by session `8476536743818790608`, but **no `--preview` in repo as of archive date** — see open items below |
| `12412840637205810569` | Implement session persistence tests | 2026-08-01 20:32 | Landed — completed session `15216642766507773818` (2026-08-01 13:47) |
| `17289339326306258117` | Story branching and merge capability | 2026-08-02 06:40 | Landed — commit `119333d` `roco story branch` |

## Task owners (kept after dedup, Aug 2 dispatch)

Sessions told to **proceed and open a PR** (one per task, to avoid conflicting PRs):

| Session ID | Task | Outcome |
|---|---|---|
| `10186746275937479489` | UX improvements §12 | completed **without pushing** — claimed done, but spinners/preview never landed (still-open §12 items; see above) |
| `1790401646888824729` | `roco story continue` | completed without pushing — **changeSet extracted and landed** in `67ef06b` |
| `7057193458828796296` | collaborative revisions (`revise`) | completed without pushing — **changeSet extracted and landed** in `67ef06b` |
| `1704905722071487336` | auto-managed session/workspace state | completed without pushing — **changeSet extracted; help-hidden part landed** in `67ef06b`, auto-resume hunk rejected (stale signature + wrong semantics) |
| `16415932402413650959` | The Warden's Folio (creative writing) | completed; PR #24 open, content landed in repo |

## Stale cross-project session (1) — replied "task closed", stopped

| Session ID | Title | Waiting since | Notes |
|---|---|---|---|
| `14294061104615164060` | Power Scaling and Documentation Refinement | 2026-03-11 | From a different repo (`CodeDoes/original_performance_takehome`, VLIW kernel optimization); delivered PR #1 (still open, unmerged) and never got a reply. Stopped 2026-08-02; session state now FAILED (closed). |

## Known open items surfaced by this archive

- **Progress spinners** (AGENTS.md §12): 3 sessions produced indicatif diffs (`904786315020180526`, `9768741472268291629`, `12167752732194728543`) — **no indicatif in repo**; the §12 owner claimed done but never pushed. Feature genuinely open.
- **`--preview` flag**: sessions `8476536743818790608`, `17981227535420543824` — **not in repo**. Open.
- **Exponential-backoff error recovery** (`crates/engine/src/retry.rs`): session `15967067688989217211` — **file absent**. Open.
- **`docs/management_intents.md`**: session `18207128461200737770` — **file absent**. Open.
- **CLI help text with management intents**: session `9784625986273857388` — not in `lib.rs` help. Open.
- **README update** (intents): session `2458249735060063710` — no root `README.md` exists. Open.
- **Warden's Folio PR #24** still open on `CodeDoes/roco_ai` (content already in repo).

## Key reference

- Jules API: https://developers.google.com/jules/api
- Live management: `scripts/jules.sh` (`check | sources | sessions | session | activities | send | create | approve | curl`)
- No delete/cancel endpoint exists in v1alpha (probed `:cancel` → 404) — archived sessions remain in the Jules web UI history.
