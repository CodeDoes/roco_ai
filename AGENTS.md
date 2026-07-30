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
Outputs prose rather than JSON for most phases — fallback parsers handle this.

## Inference Configuration

Generation is controlled by composing several knobs:

- **State-tune-bake**: Pre-load the model's recurrent state with format examples via `Bake` messages. Primes the output shape without grammar constraints.
- **BNF grammar**: Hard token-level constraints via `kbnf` masks. Forces JSON structure when the model can follow it.
- **Stop sequences**: Token sequences that halt generation (e.g. closing `}`).
- **Temperature**: Sampling randomness (0.0 = deterministic, 0.7 default).
- **Top-p / Top-k**: Nucleus + top-k sampling to filter low-probability tokens.
- **Prefill**: Seed text fed before generation (e.g. `"{\n"` to jump-start JSON).

These are composed into presets selected via `--strategy`:
- `state-tuned`: bake only, no grammar. Relies on recurrent state priming.
- `schema`: JSON schema + GBNF grammar for strict structural validation.
- `loose-json`: relaxed grammar accepting JSON-like output with minor errors.
- `grammar`: user-supplied GBNF grammar string.

When grammar constraints fail (model outputs prose despite them), a fallback
parser extracts structure from natural language output.

## State Management

Inferd maintains a **state pool** — `HashMap<String, Option<Tensor>>` with FIFO + LRU
eviction (max 8 entries). State is the model's recurrent vector after processing text.

| Operation | What it does |
|---|---|
| `state_id: "name"` | Load cached state from pool before generation |
| `save_as: "name"` | Save resulting state to pool after generation |
| `feed_eos("name")` | Load from pool, feed token 0 (EOS), save back — breaks repetition patterns |
| `Bake { text, name }` | Process text through model, save under `name`. Primes output format. |
| `save_state()` / `load_state(blob)` | Download/upload raw state tensor for persistence |

Two named states:
- `SESSION_WRITER` (`"story-writer"`): outline, wiki, chapters, synopsis
- `SESSION_VALIDATOR` (`"story-validator"`): validation only — reset between chapters

## Directory Layout

All artifacts live in a local `.roco/` directory. Override the location with the
`ROCO_DIR` environment variable. No user-level config (`~/.config/roco/`) — everything
stays local and trackable.

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

Config is loaded from `$ROCO_DIR/config.toml` if `ROCO_DIR` is set, otherwise
`.roco/config.toml`. Environment variables (`RWKV_MODEL`, `RWKV_VOCAB`) override
config file values.

## Story Pipeline

6 phases, each with fallback parsers for the 2.9B model's prose output:

```
outline → wiki → chapters (×3, each validated) → synopsis → publish
```

Prose fallback parsers handle the model's natural language output when it doesn't
produce valid JSON:

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
without a pass, the latest revision is accepted.

### Outline Data Flow

The outline handler writes full markdown to `01-OUTLINE.md`. Downstream phases read
this file directly so `chapter_outline_info()` extracts correct chapter titles and
summaries for wiki and chapter prompts.

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
validation. These prime the recurrent state to output the expected JSON format.
Bake examples use the same `System:\n\nUser:\n\nAssistant:` format as real generation.

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
with/without bake tuning, with/without validation retry.

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
| Bake API | `crates/engine-gpu/src/backend.rs` |
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
- `./docs/rfc/` — micro-RFCs
- `./USE_CASES_AND_GAPS.md` — test coverage gaps
- `./scout.sh` — live codebase introspection
