# RoCo AI

AI-assisted collaborative writing tool powered by a local LLM (RWKV-7 2.9B).

```bash
roco story "A lighthouse keeper discovers a hidden message in the fog"   # full pipeline
roco story --resume              # resume interrupted run
roco story --phase synopsis      # run one phase
roco story --fix chapter 3       # regenerate a chapter (implies --resume)
roco story --mock                # use mock backend (no real model)
roco interact                    # interactive chat
roco gui                         # desktop GUI
./run_tests.sh                   # full test suite
```

## Architecture

| Category | Crates |
|---|---|
| **Engine** | `engine` (trait/types/grammar/mock), `engine-gpu` (RwkvBackend w/ web-rwkv) |
| **Inference** | `inferd`, `infer-client` |
| **Transport** | `gateway` (unified HTTP API), `server` (standalone inferd), `protocol` |
| **Agents** | `agent` (story pipeline, ReAct loop, steering) |
| **State** | `app`, `session`, `workspace` |
| **CLI** | `cli` |
| **UI** | `ui` (desktop widgets) |
| **Testing** | `harness` |
| **GPU** | `rwkv7-vulkan` (optional) |

The engine split keeps GPU deps out of the dependency chain — `engine` has no GPU deps, `engine-gpu` only compiles when inference is needed.

## Story Pipeline

6 phases: outline → wiki → chapters → validation → synopsis → publish

Each phase tries grammar-constrained JSON first, then `repair_json()`, then a prose fallback parser:

| Phase | Fallback | Parses |
|---|---|---|
| Outline | `prose_to_outline()` | `### Title:`, `### Genre:`, `### Chapter N:` headers |
| Wiki | `prose_to_wiki()` | `Setting:`, `-**Name**: description` bullets |
| Chapter | `prose_to_chapter()` | `Title:` / `#` / `##` headers + body |
| Validation | `prose_to_validation()` | `quality:`, `issues:`, `suggestion:` fields |
| Synopsis | `prose_to_synopsis()` | `Summary:` / `Synopsis:` prefix, or raw text |

The 2.9B model outputs prose, not JSON, for all phases. Prose fallbacks convert natural output to structured data.

### Retry Cycle (generate → validate → revise → re-validate)

Each chapter follows a validation loop with up to 3 retries:

```
generate → validate → [if fail] revise → re-validate → [repeat until pass or max retries] → accept
```

The validator state is reset (`feed_eos`) before each validation to prevent cross-chapter state bleed.
Revision feedback is extracted from the validation output and fed into the next generation as context.

### State Management (inferd)

inferd receives only raw text — no `System:`/`User:`/`Assistant:` formatting, no think suppression,
no implicit session state. All formatting is the caller's responsibility:

```
CLI formats: "System: {sys}\n\nUser: {prompt}\n\nAssistant:"
inferd sees: raw text (prompt field as-is)
```

State is managed explicitly via `state_id` (load) and `save_as` (save) fields.
The old `session` / `preserve_state` API maps to `state_id` + `save_as` for backward compat.

- `feed_eos(state_name)`: loads session state, feeds token 0 (EOS), saves back to pool
- `Bake { state_id, text, name }`: loads state, processes text through model, saves under `name`

### Outline Data Flow

The outline handler writes full markdown (with chapter titles and summaries) to `01-OUTLINE.md`.
Downstream phases read this file directly, ensuring wiki, chapter prompts, and
`chapter_outline_info()` all receive the complete outline with chapter details.

## Current Status

**Tests:** 1002+ pass, 0 failures, 0 warnings across the workspace.

### Completed
- Engine split (engine vs engine-gpu)
- Gateway unification (3 deployment modes)
- Prose fallbacks for all 5 phases
- Session workspace switching, build portability script
- inferd format stripping (no System/User/Assistant in actor)
- Explicit Bake API (separate from Complete)
- FeedEos restores saved EOS-processed state (not just in-memory reset)
- Control character stripping in `clean_json_output()`
- `--fix` implies `--resume` (finds latest workspace)
- Validator state reset between chapters

### What Remains
- Crate consolidation (protocol modules, agent/validation/tools)
- Type deduplication (Agent, BakeRequest/Response across crates)
- Harness cleanup (10 identical Agent structs → generic MockAgent)
- Placeholder tests → real tests
- Real model end-to-end validation with resume and chapter quality
- `--fix` without prior workspace still exits with error (needs workspace path)

**Key gotchas:**
- Model outputs prose, not JSON → fallback chain handles this
- Control characters (NUL bytes) in output → stripped in `clean_json_output()`
- Model enters repetition loops at temperature >= 0.7 (2.9B RNN limitation)
- Outline truncation at 800 tokens → `repair_truncated_json` closes it
- Extra `}` in output → progressive suffix stripping
- `<think>` block contamination → stripped before JSON extraction
- Tool tags (`<tool_use_code>`, `<tool_use_output>`) → stripped before parsing

## Key Code

| What | Where |
|---|---|
| Story pipeline | `crates/cli/src/cmd/story.rs` (~2350 lines) |
| JSON cleaning | `crates/engine/src/grammar/strategies.rs` |
| Mock backend | `crates/engine/src/backend.rs` |
| Schema defs | `StoryOutline`, `StoryWiki`, `StoryChapter`, etc. in `story.rs` |
| Actor (inferd) | `crates/engine-gpu/src/actor.rs` |
| Bake API | `crates/engine-gpu/src/actor.rs` (`Bake` message) + `backend.rs` (`tune_state`) |
| E2E wait script | `scripts/wait-e2e.sh` |

## More Info

- `./PROGRESS.md` — current work tracking
- `./docs/rfc/` — micro-RFCs for harness, security, rollback, inference protocol
- `./docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md` — system anatomy
- `./USE_CASES_AND_GAPS.md` — test coverage gaps
- `./scout.sh` — live codebase introspection

## Environment

- **Model:** RWKV-7 2.9B (`./models/rwkv7-g1h-2.9b-...st` symlink)
- **Config:** `.roco/config.toml` or `~/.config/roco/config.toml`
- **Rust:** Edition 2021
- **GPU debug:** `RWKV_ADAPTER=llvmpipe`
