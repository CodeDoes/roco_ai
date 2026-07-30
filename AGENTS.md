# RoCo AI

AI-assisted collaborative writing tool powered by a local RWKV-7 SSM.

```bash
roco story "A lighthouse keeper discovers a hidden message in the fog"
roco story --resume              # resume from latest workspace
roco story --fix chapter 3       # regenerate a chapter (implies --resume)
roco story --phase synopsis      # run one phase
roco story --mock                # use mock backend, no real model
roco interact                    # conversational chat mode
roco gui                         # desktop GUI
./run_tests.sh                   # full test suite
```

## Philosophy

**Natural language first, CLI flags second.** The primary interaction is launching
`roco` in a workspace directory and using natural language. CLI flags exist for
automation but the design favors conversational control. `--fix` implies `--resume`
(finds the latest workspace automatically).

**Develop with `cargo run -- watch`.** Use `cargo run -- ...` or `cargo watch -x run`
during development. Debug builds are fast enough; release builds are for deployment.

## Architecture (3-tier)

```
CLI / GUI  ──→  gateway (HTTP)  ──→  inferd (token engine)
```

| Tier | Crate(s) | Responsibility |
|---|---|---|
| **Client** | `cli`, `ui` | Formats prompts, calls gateway, renders output. Owns all `System:`/`User:`/`Assistant:` formatting. |
| **Gateway** | `gateway`, `app`, `session`, `workspace` | Session management, workspace routing, state caching, request orchestration. |
| **Inferd** | `engine-gpu`, `inferd` | Pure token engine. Receives **raw text only** — no message format knowledge, no session concept. |

The engine is split (`engine` vs `engine-gpu`) to keep GPU deps out of the dependency
chain. `engine` (traits, types, mock, grammar, JSON cleaning) compiles everywhere;
`engine-gpu` (web-rwkv backend, actor) only when inference is needed.

HTTP servers:
- **gateway** (`roco gateway`): unified API on port 18000, orchestrates sessions + caching
- **inferd** (`roco-inferd`): standalone token server on 18080, spawned by gateway in dev mode
- **server** (`roco server`): standalone inferd deployment without gateway

## RWKV-7 SSM Model

RWKV-7 is a State Space Model (not a Transformer). Key properties:

- **Linear memory** in sequence length — no quadratic attention. Constant VRAM per token.
- **Trained context length**: 10240 tokens (from model filename `ctx10240`).
- **Generalizes beyond trained context** — SSMs don't have a hard context window like Transformers.
- **Quantization**: FP16 (`.st` safetensors format, `-f16` in filename).
- **Model file**: `./models/rwkv7-g1h-2.9b-20260710-ctx10240-f16.st`
- **Tokenizer**: `assets/vocab/rwkv_vocab_v20230424.json`

The 2.9B parameter model is small enough to run on consumer GPUs via Vulkan.
Outputs prose rather than JSON for most phases — the caller handles this with fallback parsers.

## Inference Configuration

The "strategy" system controls how inferd generates tokens. It's composed of several
independent knobs:

| Parameter | What it does |
|---|---|
| **State-tune-bake** | Pre-load the model's recurrent state with format examples via `Bake` messages. Primes output shape without grammar constraints. |
| **BNF grammar** | Hard token-level constraints via `kbnf` masks. Forces JSON structure when the model can follow it. |
| **Stop sequences** | Token sequences that halt generation (e.g. closing `}` to truncate at JSON boundary). |
| **Temperature** | Sampling randomness (0.0 = deterministic, 0.7 default, lower for validation). |
| **Top-p / Top-k** | Nucleus + top-k sampling to filter low-probability tokens. |
| **Prefill** | Seed text fed before generation (e.g. `"{\n"` to jump-start JSON output). |

These are composed via `StrategySelector` and `StrategyKind`:
- `state-tuned`: bake only, no grammar. Relies on recurrent state priming.
- `schema`: JSON schema + GBNF grammar for strict structural validation.
- `loose-json`: relaxed grammar that accepts JSON-like output with minor errors.
- `grammar`: user-supplied GBNF grammar string.

When grammar fails (model outputs prose despite constraints), a fallback parser
extracts structure from natural language output.

## State Management

Inferd maintains a **state pool** — `HashMap<String, Option<Tensor>>` with FIFO + LRU
eviction (max 8 entries). State is the model's recurrent vector after processing text.

| Operation | What it does |
|---|---|
| `state_id: "name"` | Load cached state from pool before generation |
| `save_as: "name"` | Save resulting state to pool after generation |
| `feed_eos("name")` | Load from pool, feed token 0 (EOS), save back — breaks repetition patterns |
| `Bake { text, name }` | Process text through model from current state, save under `name`. Used to prime output format with examples. |
| `save_state()` / `load_state(blob)` | Download/upload raw state tensor for persistence |

Two named states are managed:
- `SESSION_WRITER` (`"story-writer"`): outline, wiki, chapters, synopsis
- `SESSION_VALIDATOR` (`"story-validator"`): validation only — reset between chapters

The old `session`/`preserve_state` API is mapped to `state_id`/`save_as` for compatibility.

## Directory Layout

All artifacts live under `.roco/` in the project root. Override with `ROCO_DIR` env var.
No user-level config (`~/.config/roco/`) — keep everything local and trackable.

```
.roco/
├── config.toml              # model path, server ports, template settings
├── agent-journal.md          # runtime log (TODO: migrate to JSONL)
├── workspaces/
│   └── {timestamp}_{slug}/   # one per story run
│       ├── 01-OUTLINE.md
│       ├── 02-WIKI.md
│       ├── 03-CHAPTER_{N}.md
│       ├── 04-VALIDATION.md
│       ├── 05-SYNOPSIS.md
│       └── 06-STORY.md
└── stories/
    └── {slug}.md             # published compiled story
```

## Story Pipeline

6 phases, each with the same retry loop:

```
outline → wiki → chapters (×3, each validated) → synopsis → publish
```

The 2.9B model outputs prose, not JSON, for all phases. Each phase tries:
1. Grammar-constrained JSON (if `grammar` or `schema` strategy)
2. JSON extraction + repair (`repair_json`, `repair_truncated_json`)
3. Prose fallback parser (maps natural language to structured data)

Prose fallback parsers:

| Phase | Parses |
|---|---|
| Outline | `### Title:`, `### Genre:`, `### Chapter N:` headers |
| Wiki | `Setting:`, `-**Name**: description` bullets |
| Chapter | `Title:` / `#` / `##` headers + body |
| Validation | `quality:`, `issues:`, `suggestion:` fields |
| Synopsis | `Summary:` / `Synopsis:` prefix, or raw text |

### Chapter Validation Loop

Each chapter follows a retry cycle (up to 3 retries):

```
write → validate → [if fail] revise with feedback → re-validate → [repeat] → accept
```

Validator state is reset before each validation call. Revision feedback (Issues +
Suggestion lines) is fed into the next `prompt_revision()`. If max retries exhausted
without a pass, the latest revision is accepted to avoid infinite loops.

### Outline Data Flow

The outline handler writes full markdown to `01-OUTLINE.md`. Downstream phases read
this file directly so `chapter_outline_info()` extracts correct chapter titles and
summaries for the wiki and chapter prompts.

## Interaction Modes

- **`roco story`**: full pipeline (outline → publish). User intent is inferred from premise text.
- **`roco interact`**: chat mode — conversational steering via ReAct loop (thought → action → observation).
  The agent understands user intent and plans actions through the `MechanisticAgent` task router.
- **`roco gui`**: desktop GUI with visual workspace browser.

Action planning: `MechanisticAgent` routes tasks by `(type, domain)` pairs, e.g.
`("compose", "outline")`, `("validate", "chapter")`. Each handler is a closure
registered via `agent.register(type, domain, handler_fn)`.

## Message Format

Inferd receives raw text — no formatting. The CLI constructs the full prompt:

```rust
let prompt_text = format!("System: {}\n\nUser: {}\n\nAssistant:", system.trim(), prompt);
backend.complete(CompletionRequest { prompt: prompt_text, ... }).await;
```

System prompts per phase:

| Phase | System Prompt |
|---|---|
| Outline | `"You are a story outliner. Output valid JSON only."` |
| Wiki | `"You are a worldbuilding assistant. Output valid JSON only."` |
| Chapter | `"You are a fiction writer. Output valid JSON only."` |
| Validation | `"You are a quality reviewer. Be strict. Output valid JSON only."` |
| Synopsis | `"You are a literary summarizer. Output valid JSON only."` |

State tuning examples are baked into the model state before chapter writing and
validation. These examples show the exact JSON format expected, priming the recurrent
state without needing grammar constraints. Bake examples use the same
`System:\n\nUser:\n\nAssistant:` format as real generation.

## Quality

**Tests:** 1002+ pass, 0 failures. Run with `cargo test --workspace`.
The mock backend (`ROCO_USE_MOCK_BACKEND=1`) returns canned responses keyed on
prompt keywords for deterministic testing. Integration tests run the full pipeline
against the mock.

**E2E manual validation:** Run `./target/debug/roco story "<premise>"` against the
real model. Use `scripts/wait-e2e.sh` (polls tmux + journal) instead of guessing
sleep durations. Check workspace files for continuity and quality.

**Evals:** Not yet automated. Manual review of generated stories for coherence,
outline adherence, and prose quality.

**Ablations:** Future work — compare chapter quality with/without wiki context,
with/without bake tuning, with/without validation retry. Architecture supports
swapping these independently.

## Known Issues

### Harness Crate Duplication

`crates/harness/src/` has 10 modules (`pet.rs`, `email.rs`, `html.rs`, ...) each
defining `pub struct Agent;` implementing `DomainHarness` with identical code —
only the domain name string differs (`"pet"`, `"email"`, etc.). This was originally
meant to hold domain-specific test logic but never diverged. Should be collapsed
into a single `MockAgent` taking the domain name as a constructor parameter.

### Agent Journal Format

Currently `.roco/agent-journal.md` in Markdown. For machine processing and structured
querying, should be migrated to JSONL (one JSON object per line, with timestamp, level,
phase, message fields).

## Key Files

| What | Where |
|---|---|
| Story pipeline | `crates/cli/src/cmd/story.rs` |
| JSON cleaning + prose fallbacks | `crates/engine/src/grammar/strategies.rs` |
| Mock backend | `crates/engine/src/backend.rs` |
| Inferd actor (Bake, Complete, FeedEos) | `crates/engine-gpu/src/actor.rs` |
| Bake API + session mapping | `crates/engine-gpu/src/backend.rs` |
| Gateway HTTP routes | `crates/gateway/src/lib.rs` |
| Inferd server | `crates/inferd/src/main.rs` |
| Protocol types | `crates/protocol/src/lib.rs` |
| Workspace management | `crates/workspace/` |
| Session management | `crates/session/` |
| Config | `.roco/config.toml` |
| Model | `./models/rwkv7-g1h-2.9b-20260710-ctx10240-f16.st` |
| Tokenizer | `assets/vocab/rwkv_vocab_v20230424.json` |
| E2E wait script | `scripts/wait-e2e.sh` |

## More Info

- `./PROGRESS.md` — current work tracking
- `./docs/rfc/` — micro-RFCs for harness, security, rollback, inference protocol
- `./docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md` — system anatomy
- `./USE_CASES_AND_GAPS.md` — test coverage gaps
- `./scout.sh` — live codebase introspection
