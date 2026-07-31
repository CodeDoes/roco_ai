# RoCo AI — System Onboarding

AI-assisted collaborative writing tool. Backend: RWKV-7 SSM (2.9B params, 10k trained context). Think of it as an agentic scaffold around a recurrent state-space language model — the model generates tokens, everything else is orchestration, formatting, and state management.

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

- **inferd** (`crates/inferd/` + `crates/engine-gpu/`): loads the model into VRAM (via Vulkan), accepts a prompt → runs inference → returns generated text. It knows nothing about messages, sessions, or user personas. It receives: text, state_id, grammar, temperature, max_tokens, stop. It returns: text.

- **frontend** (`crates/cli/`, `crates/gateway/`, `crates/app/`, `crates/ui/`): orchestrates the pipeline. Decides what text to send, what state to load/save, which grammar to apply, and what to do with the output.

RWKV-7 is a **State Space Model** (not a Transformer). Recurrent — linear memory in sequence length, constant VRAM per token. Because it's recurrent, "session" isn't a connection — it's a **vector of floats** (the model's latent state after processing N tokens). Save that vector → you've saved the session. Load it into the model → you're continuing from where you left off.

## 2. Directory Layout — What Goes Where and Why

Every command works relative to the current directory. All artifacts go in `.roco/` under that directory. That means:

- You `cd` into a project directory → `.roco/` is your project's scratch space
- You can have different `.roco/` in different directories → isolated projects
- No files leak into `~/.config/roco/` or `~/.local/share/roco/`
- The entire project state is a single directory you can delete or archive

Override `.roco/` location with `$ROCO_DIR`. If `ROCO_DIR=/data/roco`, then config loads from `/data/roco/config.toml`, workspaces under `/data/roco/workspaces/`, stories under `/data/roco/stories/`. This is for headless/CI/docker setups where cwd isn't meaningful.

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

Split ensures `engine` compiles everywhere (CI, docs, wasm targets). GPU deps only enter the build when `engine-gpu` is a dependency — which only happens when you need real inference. Tests compile against `engine` alone using `MockBackend`.

## 4. inferd Architecture — Atomic Operations

inferd's actor loop handles these operations:

| Operation | Effect |
|---|---|
| `Complete { text, init_state, state_slot, grammar, temp, max_tokens }` | Load `init_state` from cache (or blank if None), generate tokens from `text`, save resulting state as `state_slot` (or don't cache if None), return output text |
| `Bake { text, init_state, state_slot }` | Load `init_state` (or blank), feed `text` through model (no generation), save resulting state as `state_slot`. Primes the recurrent state. |
| `SaveState / LoadState` | Serialize/deserialize the raw state tensor for persistence. |

FeedEOS is NOT an inferd primitive. Callers manage state reset by saving/loading from cache or baking EOS text directly.

State pool: `HashMap<String, Option<Tensor>>`. Max 8 entries. FIFO + LRU eviction. Thread-safe (behind `Arc<RwLock<...>>`).

## 5. Inference Parameters

There is no single "strategy" — there are knobs that get composed:

| Knob | What it does |
|---|---|
| **Grammar** | BNF grammar string → `kbnf`-compiled token mask. Forces token sequences to match a grammar (e.g. valid JSON). |
| **Temperature** | Sampling temperature. 0.0 = greedy (always pick highest probability). Our 2.9B model starts repeating at ≥0.7. |
| **Top-p** | Nucleus sampling. Only sample from tokens whose cumulative probability exceeds p. |
| **Top-k** | Only sample from the k highest-probability tokens. |
| **Prefill** | Initial tokens to feed before sampling. For JSON, `"{\n"` jump-starts the output. |
| **Stop** | Token sequences that halt generation (e.g. `"}\n"` for JSON). |
| **Bake** (state tune) | Feed few-shot examples through the model (no generation) to prime the recurrent state with format expectations. |

The CLI `--strategy` flag selects a preset composition of these knobs:
- `state-tuned`: bake examples with no grammar, rely on recurrent state
- `schema`: JSON schema grammar
- `loose-json`: relaxed grammar, accepts JSON-like output
- `grammar`: user-supplied grammar string

## 6. State Management — Sessions Are Vectors

"Session" in RWKV is the model's recurrent state — a vector of floats. You manage it explicitly with `state_id` (load) and `save_as` (save).

Two named states used by the pipeline:

| Name | Used for |
|---|---|
| `story-writer` | Outline, wiki, chapters, synopsis — accumulates the full context |
| `story-validator` | Validation only — reset between chapters to prevent repetition bleed |

The state cache is a performance shortcut; the canonical state is the JSONL interaction log.

## 7. Story Pipeline — 6 Phases

```
outline → wiki → chapter (×N, each validated) → synopsis → publish
```

Each phase:
1. Formulates a prompt with `System:` preamble, `User:` task, `Assistant:` prefix
2. Sends to inferd as raw text
3. Attempts to parse the output as JSON (with grammar-constrained generation)
4. If JSON parsing fails, the phase should **fail loudly** — no prose fallback

## 8. Prompt Format — Who Formats What

inferd does NOT format prompts. It receives raw text and generates more text. The pipeline constructs:

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

Testing and validation have the highest priority. Prose fallback parsers were deleted in commit f3bab52. They existed because I violated the correct workflow: **evals first, workarounds never.**

A JSON parse failure is a system bug — handle it by fixing the system (prompts, baking, grammar, temperature), never by parsing prose.

### The validation loop (repeat until green)

```
test → eval → check + update PROGRESS.md with what you want to do →
e2e (story) → note problems in PROGRESS.md → fix issues →
targeted-e2e (chapter/wiki/outline/validate) → update PROGRESS.md →
consider whether a lack of info within AGENTS.md caused this problem,
if so update AGENTS.md → repeat
```

Steps in detail:

1. **Tests** (unit + integration) — prove the code works correctly in isolation (mock backend, deterministic, fast). `cargo check --workspace --all-targets` + `cargo test --workspace` must be green before touching the real model.
2. **Evals** — prove the model CAN produce correct output (valid JSON, coherent chapters) given correct inputs from the fixed pipeline. Each phase tested separately against the real model. Use `cargo run --release --example eval_suite -p roco-cli --features net -- http://127.0.0.1:18080 <case>`.
3. **Check + update PROGRESS.md with what you want to do** — BEFORE running the pipeline, update PROGRESS.md with your intent for this iteration: the specific change/experiment you're about to run, what you expect to happen, and what you're looking for. This makes the iteration auditable — if the run fails, the log shows what was expected vs what happened.
4. **E2E (full story)** — run `roco story` end-to-end with the real model. Manually review output quality.
5. **Note problems in PROGRESS.md** — every failure (model behavior, pipeline bug, harness bug) goes in the log, with evidence from logs/artifacts.
6. **Fix issues** — one at a time. Verify with cargo check after each change.
7. **Targeted E2E** — re-run only the affected phase(s) (chapter/wiki/outline/validate) against the real model to confirm the fix without a full pipeline run.
8. **Update PROGRESS.md** — mark what's verified green, what's still red.
9. **AGENTS.md self-check** — consider whether a lack of info within AGENTS.md caused this problem (e.g. a documented invariant that wasn't documented, or a note that would have prevented the mistake). If so, update AGENTS.md so the next iteration doesn't repeat it.
10. **Repeat** until the loop exits clean.

### Known Phase 8 verification state

As of the latest full E2E run (2026-07-31), the complete story pipeline passes against the real model:

- Grammar-constrained decoding fixed: `bnf_engine.rs` converts GBNF → kbnf (appends `;` per rule)
- Gateway `/v1/completions` forwards `grammar`/`prefill`/`init_state`/`state_slot`/`seed`
- **Schema enum scoping fixed** (`json_schema.rs`): enums are emitted as named rules, never inlined into object rules (GBNF `|` precedence trap — see §10)
- **Generation loop budget fixed** (`engine-gpu/src/actor.rs`): the first loop samples ONE token then hands off to the main loop (previously ran to `max_tokens` AND the main loop ran `max_tokens−1` more = 2×max_tokens−1 tokens). Grammar-closing token emitted before stopping in both loops.
- Full E2E: outline → wiki → chapters 1-3 (validated) → synopsis → publish ✅

## 10. Known Legacy Issues

### Agent Journal Format
Currently `.roco/agent-journal.md` in Markdown. Should be JSONL for structured querying (one JSON object per line: timestamp, level, phase, message).

### GBNF `|` precedence — never inline enum alternations into larger rules
`|` has the LOWEST precedence in GBNF/kbnf. Inlining an enum (`"pass" | "fail" | "needs-work"`) into an object rule splits the whole rule into multiple alternatives — the model can then legally emit a bare enum value (`"needs-work","suggestion":...`) that skips the `{`, the key, and sibling fields. `schema_to_gbnf` now emits enums as named rules (`root_quality_enum ::= ...`) and references them. If you hand-write GBNF with an enum inside an object, wrap it in a named rule.

### Generation loop structure (engine-gpu/src/actor.rs)
The complete() path has two loops: the first flushes the prompt and samples the FIRST token, then the main loop generates the rest. Both must share the same sampling path (`sampling::sample_token_masked_with_rng`) so grammar handling stays consistent. The grammar-closing token is emitted BEFORE the break (append-then-check), never dropped.

### Prefill vs grammar mask
Prefill tokens (`{\n`) are fed to the model but deliberately NOT passed through the grammar mask. The mask re-emits `{` as its first generated token, so `resp.text` is complete JSON on its own — callers parse `resp.text` directly without prepending the prefill. Do NOT "fix" this by accepting prefill tokens into the mask; it breaks the pipeline's parse path.

### Model-Specific Behavior
2.9B parameter RWKV-7 model:
- Starts repeating at temperature ≥ 0.7 (empirical)
- Can produce NUL/control characters in output (stripped by `clean_json_output`)
- Sometimes produces ````json``` wrappers or trailing `}` characters
- May truncate output when the state carries too much context momentum
- **Judges instruction-following too literally** (e.g. "crystal sword" ≠ "ancient artifact" → `follows_instructions: false`). The `val_instruction_following_matched` eval fails for this reason — it's a model limitation, not a system bug.
- **In-string content degrades under tight constraints** — with a grammar mask active, string values can contain degenerate tokens (`":[`, `s,s,s`). Structure is always valid JSON; content quality varies. Revision retries handle the worst cases.
- **Chapter 2-3 often need 2-3 revision retries** at temp 0.5 — the retry loop converges, this is expected.

### Migration Complete

The legacy `session`/`bake_state`/`OpenAiCompletionRequest::session` bridge fields have been removed. All callers use `init_state`/`state_slot` and embed system text directly in the prompt.
