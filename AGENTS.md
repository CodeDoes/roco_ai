# RoCo AI — System Onboarding

AI-assisted collaborative writing tool. Backend: RWKV-7 SSM (2.9B params, 10k
trained context). Think of it as an agentic scaffold around a recurrent
state-space language model — the model generates tokens, everything else is
orchestration, formatting, and state management.

```bash
roco story "A lighthouse keeper discovers a hidden message in the fog"
roco story --resume              # find and continue latest workspace
roco story --fix chapter 3       # regenerate chapter 3 in latest workspace
roco story --phase synopsis      # run a single phase, resume from there
roco story --mock                # mock backend, no real model (tests)
roco gui                         # desktop app
```

## 1. What This Thing Actually Is

Two processes communicating over HTTP:

```
frontend (CLI / GUI / gateway)  ←→  inferd (token engine)
```

- **inferd** (`crates/inferd/` + `crates/engine-gpu/`): loads the model into VRAM
  (via Vulkan), accepts a prompt → runs inference → returns generated text. It
  knows nothing about messages, sessions, or user personas. It receives: text,
  state_id, grammar, temperature, max_tokens, stop. It returns: text.

- **frontend** (`crates/cli/`, `crates/gateway/`, `crates/app/`, `crates/ui/`):
  orchestrates the pipeline. Decides what text to send, what state to load/save,
  which grammar to apply, and what to do with the output.

RWKV-7 is a **State Space Model** (not a Transformer). Recurrent — linear memory
in sequence length, constant VRAM per token. Because it's recurrent, "session"
isn't a connection — it's a **vector of floats** (the model's latent state after
processing N tokens). Save that vector → you've saved the session. Load it into
the model → you're continuing from where you left off.

## 2. Directory Layout — What Goes Where and Why

Every command works relative to the current directory. All artifacts go in
`.roco/` under that directory. That means:

- You `cd` into a project directory → `.roco/` is your project's scratch space
- You can have different `.roco/` in different directories → isolated projects
- No files leak into `~/.config/roco/` or `~/.local/share/roco/`
- The entire project state is a single directory you can delete or archive

Override `.roco/` location with `$ROCO_DIR`. If `ROCO_DIR=/data/roco`, then
config loads from `/data/roco/config.toml`, workspaces under `/data/roco/workspaces/`,
stories under `/data/roco/stories/`. This is for headless/CI/docker setups where
cwd isn't meaningful.

```
.roco/                              # ROCO_DIR env var overrides this
├── config.toml                     # model path, server ports, template settings
├── agent-journal.md                # runtime log (TODO: migrate to JSONL)
├── workspaces/
│   └── {timestamp}_{slug}/
│       ├── 01-OUTLINE.md
│       ├── 02-WIKI.md
│       ├── 03-CHAPTER_*.md
│       ├── 04-VALIDATION.md
│       ├── 05-SYNOPSIS.md
│       └── 06-STORY.md
└── stories/
    └── {slug}.md                   # compiled final story
```

## 3. Engine Split — Why Two Crates

| Crate | Deps | Responsibility |
|---|---|---|
| `engine` | pure Rust, no GPU | Traits (`ModelBackend`), types (`CompletionRequest`, `TokenizeResult`), mock backend, grammar strategies, JSON helpers |
| `engine-gpu` | `web-rwkv` (Vulkan) | `RwkvActor` (tokio actor managing model), `RwkvBackend` (implements `ModelBackend`) |

Split ensures `engine` compiles everywhere (CI, docs, wasm targets). GPU deps
only enter the build when `engine-gpu` is a dependency — which only happens when
you need real inference. Tests compile against `engine` alone using `MockBackend`.

## 4. inferd Architecture — Atomic Operations

inferd's actor loop handles these operations:

| Operation | Effect |
|---|---|
| `Complete { text, init_state, state_slot, grammar, temp, max_tokens }` | Load `init_state` from cache (or blank if None), generate tokens from `text`, save resulting state as `state_slot` (or don't cache if None), return output text |
| `Bake { text, init_state, state_slot }` | Load `init_state` (or blank), feed `text` through model (no generation), save resulting state as `state_slot`. Primes the recurrent state. |
| `SaveState / LoadState` | Serialize/deserialize the raw state tensor for persistence. |

FeedEos is NOT an inferd primitive. Callers manage state reset by saving/loading from cache or baking EOS text directly.

State pool: `HashMap<String, Option<Tensor>>`. Max 8 entries. FIFO + LRU
eviction. Thread-safe (behind `Arc<RwLock<...>>`).

## 5. Inference Parameters

There is no single "strategy" — there are knobs that get composed:

| Knob | What it does |
|---|---|
| **Grammar** | BNF grammar string → `kbnf`-compiled token mask. Forces token sequences to match a grammar (e.g. valid JSON). |
| **Temperature** | Sampling temperature. 0.0 = greedy (always pick highest probability). Our 2.9B model starts repeating at ≥0.7. |
| **Top-p** | Nucleus sampling. Only sample from tokens whose cumulative probability exceeds p. |
| **Top-k** | Only sample from the k highest-probability tokens. |
| **Prefill** | Initial tokens to feed before sampling begins. For JSON, `"{\n"` jump-starts the output. |
| **Stop** | Token sequences that halt generation (e.g. `"}\n"` for JSON). |
| **Bake (state tune)** | Feed few-shot examples through the model (no generation) to load the recurrent state with format expectations. |

The CLI `--strategy` flag selects a preset composition of these knobs:
- `state-tuned`: bake examples with no grammar, rely on recurrent state
- `schema`: JSON schema grammar
- `loose-json`: relaxed grammar, accepts JSON-like output
- `grammar`: user-supplied grammar string

## 6. State Management — Sessions Are Vectors

"Session" in RWKV is the model's recurrent state — a vector of floats. You
manage it explicitly with `state_id` (load) and `save_as` (save).

Two named states used by the pipeline:

| Name | Used for |
|---|---|
| `story-writer` | Outline, wiki, chapters, synopsis — accumulates the full context |
| `story-validator` | Validation only — reset between chapters to prevent bleed |

### Why feed_eos between operations

The model's state carries momentum from everything it processed. Generating a
long chapter leaves the state primed to keep writing prose. If you then validate
without resetting, the validation result is contaminated by chapter-writing
momentum. `feed_eos` forces the state through one EOS token, which breaks
the repetition/continuation pattern without losing the state's knowledge of
the story context.

## 7. Story Pipeline — 6 Phases

```
outline → wiki → chapter (×N, each validated) → synopsis → publish
```

Each phase:
1. Formulates a prompt with `System:` preamble, `User:` task, `Assistant:` prefix
2. Sends to inferd as raw text (formatting is done by the pipeline, not inferd)
3. Attempts to parse the output as JSON (with grammar-constrained generation)
4. If JSON parsing fails, the phase should **fail loudly** — not silently fall back
   to natural language heuristics

### Chapter Validation Loop

```
write → validate → [if fail] revise with feedback → re-validate
                   → [if pass] accept
                   → [if max retries] accept latest revision anyway
```

Key detail: validator state is cleaned with `feed_eos` before each call.
Validator has its own session to prevent writer state from bleeding into
critique.

### Outline Data Flow

Handler writes full markdown to `01-OUTLINE.md`. Downstream phases read this
file. This is explicit — the outline text is persisted to disk so it can be
re-read by any phase, not passed through in-memory.

## 8. Prompt Format — Who Formats What

inferd does NOT format prompts. It receives raw text and generates more text.
The pipeline constructs:

```rust
let full_text = format!("System: {}\n\nUser: {}\n\nAssistant:", system, prompt);
inferd.complete(CompletionRequest {
    prompt: full_text,
    state_id: Some("story-writer"),
    save_as: Some("story-writer"),
    grammar: json_schema_grammar(),
    temperature: 0.6,
    max_tokens: 500,
    // ...
})
```

System prompts are task-specific:

| Phase | System prompt |
|---|---|
| Outline | `"You are a story outliner. Output valid JSON only."` |
| Wiki | `"You are a worldbuilding assistant. Output valid JSON only."` |
| Chapter | `"You are a fiction writer. Output valid JSON only."` |
| Validation | `"You are a quality reviewer. Be strict. Output valid JSON only."` |
| Synopsis | `"You are a literary summarizer. Output valid JSON only."` |

## 9. Eval-First Methodology — The Correct Order

Prose fallback parsers should NOT exist. They were deleted in commit f3bab52.
They existed because I violated the correct workflow: **evals first, workarounds
never.**

The correct order:
1. **Tests** (unit + integration) — prove the code works correctly in isolation
   (mock backend, deterministic, fast).
2. **Evals** — prove the model CAN produce correct output (valid JSON, coherent
   chapters) given correct inputs from the fixed pipeline. Each phase tested
   separately against the real model.
3. **Manual E2E** — run the full pipeline, manually review output quality.

Prose fallback parsers were added before step 2, masking bugs that should have
been fixed by tuning prompts/baking/grammar. A JSON parse failure is a system
bug — handle it by fixing the system, not by parsing prose.

As of this writing (post-fix), evals have NOT been run on the corrected system.
They should be run next to confirm each phase produces valid JSON.

### Existing Eval Infrastructure

| File | What it tests |
|---|---|
| `crates/engine/src/story_evals.rs` | Each pipeline stage: outline, wiki, chapter, validation, revision |
| `crates/cli/examples/format_eval.rs` | 8 format specs × baked/fresh × N trials, measures format compliance, repetition, sensory density |
| `crates/cli/examples/matrix_eval.rs` | Multi-parameter matrix eval |
| `crates/cli/src/cmd/eval_suite.rs` | CLI command for running eval suites |
| `evals/format_eval.sh`, `evals/grammar_eval.sh` | Shell runners |
| `evals/results/` | JSON and Markdown reports from previous runs |

## 10. Known Legacy Issues

### Harness Crate Duplication

`crates/harness/src/` has 10 modules (`pet.rs`, `email.rs`, `html.rs`, ...) each
defining `pub struct Agent;` with identical `DomainHarness` impl — only the
domain name string differs. Originally meant to hold domain-specific test
fixtures but the logic never materialized. Should be collapsed into a single
`MockAgent` that takes domain name as constructor parameter.

### Agent Journal Format

Currently `.roco/agent-journal.md` in Markdown. Should be JSONL for structured
querying (one JSON object per line: timestamp, level, phase, message).

### Model-Specific Behavior

2.9B parameter RWKV-7 model:
- Starts repeating at temperature ≥ 0.7 (empirical)
- Can produce NUL/control characters in output (stripped by `clean_json_output`)
- Sometimes produces ````json``` wrappers or trailing `}` characters
- May truncate output when the state carries too much context momentum

## 11. Key Source Files

| Component | Path |
|---|---|
| Story pipeline | `crates/cli/src/cmd/story.rs` |
| Inferd actor (Complete, Bake, FeedEos) | `crates/engine-gpu/src/actor.rs` |
| Inferd backend (maps old→new API) | `crates/engine-gpu/src/backend.rs` |
| Mock backend | `crates/engine/src/backend.rs` |
| JSON helpers + grammar types | `crates/engine/src/grammar/strategies.rs` |
| Protocol types | `crates/protocol/src/lib.rs` |
| Gateway HTTP routes | `crates/gateway/src/lib.rs` |
| Story evals (per-stage model tests) | `crates/engine/src/story_evals.rs` |
| Format eval (multi-format comparison) | `crates/cli/examples/format_eval.rs` |
| Model file | `./models/rwkv7-g1h-2.9b-20260710-ctx10240-f16.st` |
| Tokenizer vocab | `assets/vocab/rwkv_vocab_v20230424.json` |
| E2E wait script | `scripts/wait-e2e.sh` |
| Eval runners | `evals/*.sh` |
| Config template | `.roco/config.toml` example |
