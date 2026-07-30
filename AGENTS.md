# RoCo AI

AI-assisted collaborative writing tool powered by a local LLM (RWKV-7 2.9B).

```bash
roco story "A lighthouse keeper discovers a hidden message in the fog"   # full pipeline
roco story --resume              # resume interrupted run
roco story --phase synopsis      # run one phase
roco story --fix chapter 3       # regenerate a chapter
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
| Validation | — | JSON only (model outputs this reliably) |
| Synopsis | — | JSON only (model outputs this reliably) |

The 2.9B model outputs prose, not JSON, for outline/wiki/chapter phases. Prose fallbacks convert natural output to structured data.

## Current Status

**Tests:** 1002 pass, 0 failures, 0 warnings across the workspace.

**Completed:**
- Engine split (engine vs engine-gpu)
- Gateway unification (3 deployment modes)
- Prose fallbacks for all 3 prose-prone phases
- Session workspace switching, build portability script

**What remains:**
- Crate consolidation (protocol modules, agent/validation/tools)
- Type deduplication (Agent, BakeRequest/Response across crates)
- Harness cleanup (10 identical Agent structs → generic MockAgent)
- Placeholder tests → real tests
- Real model end-to-end validation with resume and chapter quality

**Key gotchas:**
- Model outputs prose, not JSON → fallback chain handles this
- Outline truncation at 800 tokens → `repair_truncated_json` closes it
- Extra `}` in output → progressive suffix stripping
- `<think>` block contamination → stripped before JSON extraction
- Tool tags (`<tool_use_code>`, `<tool_use_output>`) → stripped before parsing

## Key Code

| What | Where |
|---|---|
| Story pipeline | `crates/cli/src/cmd/story.rs` (~2200 lines) |
| JSON cleaning | `crates/engine/src/grammar/strategies.rs` |
| Mock backend | `crates/engine/src/backend.rs` |
| Schema defs | `StoryOutline`, `StoryWiki`, `StoryChapter`, etc. in `story.rs` |

## Current Work

See `./PROGRESS.md` for what's being worked on right now.

## More Info

- `./docs/rfc/` — micro-RFCs for harness, security, rollback, inference protocol
- `./docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md` — system anatomy
- `./USE_CASES_AND_GAPS.md` — test coverage gaps
- `./scout.sh` — live codebase introspection

## Environment

- **Model:** RWKV-7 2.9B (`./models/rwkv7-g1h-2.9b-...st` symlink)
- **Config:** `.roco/config.toml` or `~/.config/roco/config.toml`
- **Rust:** Edition 2021
- **GPU debug:** `RWKV_ADAPTER=llvmpipe`
