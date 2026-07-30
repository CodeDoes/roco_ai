# RoCo AI

AI-assisted collaborative writing tool powered by a local LLM (RWKV-7 2.9B).

```bash
roco story "A lighthouse keeper discovers a hidden message in the fog"
roco story --resume
roco story --phase synopsis
roco story --fix chapter 3       # implies --resume
roco story --mock
roco interact
roco gui
```

## Philosophy

**Natural language first, CLI flags second.** The primary interaction mode is launching
`roco` in a workspace and using natural language to drive the story forward. CLI flags
(`--phase`, `--fix`, `--resume`) exist for automation but the design favors conversational
control. `--fix` without `--workspace` implies `--resume` (finds the latest workspace).

**Develop with `cargo run -- watch`.** During development, use `cargo run -- ...` or
`cargo watch -x run` rather than building release binaries. The debug build is fast enough
for iteration; release builds are for deployment.

## Architecture (3-tier)

```
┌─────────────┐     ┌──────────┐     ┌─────────┐
│  CLI / GUI  │ ──→ │ gateway  │ ──→ │ inferd  │
│  (client)   │     │ (HTTP)   │     │ (token  │
│             │ ←── │          │ ←── │ engine) │
└─────────────┘     └──────────┘     └─────────┘
```

| Tier | Crate(s) | Responsibility |
|---|---|---|
| **Client** | `cli`, `ui` | Formats prompts, calls gateway, renders output. Owns all `System:`/`User:`/`Assistant:` formatting. |
| **Gateway** | `gateway`, `app`, `session`, `workspace` | Manages sessions, workspaces, caches state, routes requests to inferd. The sole orchestration layer. |
| **Inferd** | `engine-gpu`, `inferd` | Pure token engine. Receives **raw text only** — no message format knowledge, no session concept. |

The engine crate is split from engine-gpu so GPU deps never enter the dependency chain unless
inference is needed. `engine` (traits, types, mock, grammar, JSON cleaning) compiles everywhere;
`engine-gpu` (web-rwkv backend, actor) only when `--features gpu` is active.

HTTP servers:
- **gateway** (`roco gateway`): the unified API, runs on port 18000 by default. Orchestrates sessions, caches, and routes to inferd.
- **inferd** (`roco-inferd`): standalone token server, runs on 18080. Gateway spawns it automatically in dev mode.
- **server** (`roco server`): standalone inferd deployment without gateway.

## State Management

All state lives in inferd's **state pool** — a `HashMap<String, Option<Tensor>>` with FIFO + LRU eviction (max 8 entries).

| Operation | What it does |
|---|---|
| `state_id: "name"` | Load cached state from pool before generation |
| `save_as: "name"` | Save resulting state to pool after generation |
| `feed_eos("name")` | Load from pool, feed token 0 (EOS), save back — breaks repetition patterns |
| `Bake { state_id, text, name }` | Load state, process text through model, save under `name` |
| `save_state()` / `load_state(blob)` | Download/upload raw state tensor for persistence |

Every `complete()` call with `session` set maps to both `state_id` and `save_as` — the old
implicit persistence is preserved for backward compat. The `Bake` message is used for explicit
state tuning (priming output format with examples).

Inferd has no session concept — sessions are a gateway concern. The gateway manages two named
states:
- `SESSION_WRITER` (`"story-writer"`): used for outline, wiki, chapters, synopsis
- `SESSION_VALIDATOR` (`"story-validator"`): used for validation only, reset between chapters

Workspace management lives in the `workspace` crate — each story run gets a timestamped
directory under `.roco/workspaces/` holding `01-OUTLINE.md` through `06-STORY.md`.
Jobs (pipeline phases) are managed by the `agent` crate's `MechanisticAgent`, which dispatches
`Task` structs to registered handlers.

## Message Format

Inferd receives **no message formatting**. The entire `"System: ...\n\nUser: ...\n\nAssistant:"`
structure is constructed by the CLI before sending:

```rust
// In structured_complete_with_strategy():
let prompt_text = format!("System: {}\n\nUser: {}\n\nAssistant:", system.trim(), prompt);
backend.complete(CompletionRequest {
    prompt: prompt_text,  // raw text, no further wrapping
    system: String::new(), // deprecated, actor ignores
    ...
}).await;
```

System instructions per phase:

| Phase | System Prompt |
|---|---|
| Outline | `"You are a story outliner. Output valid JSON only."` |
| Wiki | `"You are a worldbuilding assistant. Output valid JSON only."` |
| Chapter | `"You are a fiction writer. Output valid JSON only."` |
| Validation | `"You are a quality reviewer. Be strict. Output valid JSON only."` |
| Synopsis | `"You are a literary summarizer. Output valid JSON only."` |

State tuning examples (bake) use the **same** formatting as real generation. Before chapter
writing starts, the writer state is baked with `BAKE_CHAPTER_EXAMPLES` (fantasy + sci-fi
examples showing the `{"title": ..., "content": ...}` JSON format). The validator state
is baked with `BAKE_VALIDATION_EXAMPLES` showing `{"quality": ..., "issues": ..., "suggestion": ...}`.
This primes the model's recurrent state to output JSON without needing grammar constraints.

## Story Pipeline

6 phases, each with JSON-first → `repair_json()` → prose-fallback chain:

```
outline → wiki → chapters (×3, each validated) → synopsis → publish
```

| Phase | Prose Fallback | Parses |
|---|---|---|
| Outline | `prose_to_outline()` | `### Title:`, `### Genre:`, `### Chapter N:` |
| Wiki | `prose_to_wiki()` | `Setting:`, `-**Name**: description` |
| Chapter | `prose_to_chapter()` | `Title:` / `#` / `##` headers + body |
| Validation | `prose_to_validation()` | `quality:`, `issues:`, `suggestion:` |
| Synopsis | `prose_to_synopsis()` | `Summary:` / `Synopsis:` prefix, or raw text |

The 2.9B model outputs prose for ALL phases. Fallback parsers convert natural language to
structured types. The outline data flow is: handler writes full markdown chapter list to
`01-OUTLINE.md`; downstream phases read that file directly so `chapter_outline_info()`
extracts correct chapter titles and summaries.

### Chapter Validation Loop

Each chapter follows a retry cycle (up to 3 retries):

```
write → validate → [if fail] revise with feedback → re-validate → [repeat] → accept
```

Validator state is reset (`feed_eos(SESSION_VALIDATOR)`) before each validation call,
preventing cross-chapter and cross-retry state bleed. Revision feedback (Issues + Suggestion
lines from validation) is fed into the next `prompt_revision()` call. If max retries are
exhausted without a pass, the latest revision is accepted to avoid infinite loops.

## Engineering

**Build profiles:** `cargo build` (debug) for development, `cargo build --release` for
deployment. Debug mode is fast enough for iteration — use `cargo watch -x run` during
development rather than rebuilding release.

**Code split rationale:** The engine split (`engine` vs `engine-gpu`) isolates GPU deps
(web-rwkv, vulkan) so that non-inference crates (CLI, agent, gateway) compile without
GPU toolchain. The compilation decision is a tradeoff: more crates = more compilation
units but cleaner dep boundaries.

**Model location:** `./models/rwkv7-g1h-2.9b-...st` symlink. Quantization is the `.st`
(safetensors) format — the model uses the default quantized state for RWKV-7 (likely
FP16 or int8, determined by the model file).

## Quality

**Tests:** 1002+ pass, 0 failures, 0 warnings. Run with `cargo test --workspace`.
The mock backend (`ROCO_USE_MOCK_BACKEND=1`) returns canned JSON responses keyed on
prompt keywords for deterministic testing without a real model. Integration tests
(`mock_cli_subcommands`) run the full pipeline against the mock backend.

**E2E manual validation:** Run `./target/debug/roco story "<premise>"` against the real
model. Use `scripts/wait-e2e.sh` to poll for completion instead of guessing sleep
durations. Check workspace files for continuity and quality.

**Ablations:** Future work — compare chapter quality with/without wiki context, with/without
bake tuning, with/without validation retry. The architecture supports swapping these
components independently.

**Evals:** Not yet automated. Manual review of generated stories for coherence, outline
adherence, and prose quality. The `04-VALIDATION.md` file records the model's own quality
assessment for each chapter.

## Interaction Modes

- **`roco story`**: full pipeline (outline → publish). User intent is inferred from the premise text.
- **`roco interact`**: chat mode — conversational steering of the agent. The agent uses ReAct
  loop (thought → action → observation) to understand user intent and plan actions.
- **`roco gui`**: desktop GUI — same pipeline with visual workspace browser.
- **Action planning**: the `MechanisticAgent` routes tasks by `(type, domain)` pairs
  (e.g. `("compose", "outline")`, `("validate", "chapter")`). Each handler is a closure
  registered via `agent.register(type, domain, handler_fn)`.

## Key Files

| What | Where |
|---|---|
| Story pipeline (all phases) | `crates/cli/src/cmd/story.rs` (~2350 lines) |
| JSON cleaning + prose fallbacks | `crates/engine/src/grammar/strategies.rs` |
| Mock backend | `crates/engine/src/backend.rs` |
| Inferd actor (Bake, Complete, FeedEos) | `crates/engine-gpu/src/actor.rs` |
| Bake API + session mapping | `crates/engine-gpu/src/backend.rs` |
| Gateway HTTP routes | `crates/gateway/src/lib.rs` |
| Inferd server | `crates/inferd/src/main.rs` |
| Protocol types | `crates/protocol/src/lib.rs` |
| Workspace management | `crates/workspace/` |
| Session management | `crates/session/` |
| E2E wait script | `scripts/wait-e2e.sh` |
| Configuration | `.roco/config.toml` or `~/.config/roco/config.toml` |
| Model | `./models/rwkv7-g1h-2.9b-...st` symlink |

## More Info

- `./PROGRESS.md` — current work tracking
- `./docs/rfc/` — micro-RFCs for harness, security, rollback, inference protocol
- `./docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md` — system anatomy
- `./USE_CASES_AND_GAPS.md` — test coverage gaps
- `./scout.sh` — live codebase introspection
