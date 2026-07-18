# AGENTS.md — RoCo AI

> Operational manual for working in this repo.

## What this is

A Rust workspace whose only inference path is `crates/inference/src/backend.rs`
(RWKV-7 via `web-rwkv` + WGPU). The repo has been pared down to the
local-RWKV critical path and restructured into focused crates — the
`crates/inference` library plus `crates/grammar`, `crates/bnf-engine`,
`crates/engine`, and the supporting crates (`message`, `tools`,
`session`, `workspace`, `agent`, `chat-common`, `cli`, `tui`,
`server`, `gateway`), the `vendor/web-rwkv` patch, the `scripts/`
model converters, and the `assets/vocab` tokenizer. Everything non-RWKV
(orchestrator crates, gateway/web frontends, Docker, agent/eval
scaffolding) has been removed; git history preserves it.

## Primary Goal

**A collaborative story writing tool where humans and AI work together to create stories.**

The human is the author. The AI is the tool. Every feature should amplify human creativity, not replace it.

See `goals/story-engine/index.md` for the detailed roadmap.

## Core Philosophy: Human Controls Pace, Not Reviews Output

**The human should not be burdened with reviewing everything.**

Instead of:
- Agent generates everything → Human reviews everything → Agent revises everything

Do this:
- Agent completes **one task** → Human sees result → Human decides: accept, modify, skip, stop

This is a conversation, not a review process. The human controls the pace by:
- **Accepting** — move to next task
- **Modifying** — give feedback, agent revises
- **Skipping** — jump ahead
- **Stopping** — end and publish

No mandatory review. No approval gates. Just natural flow.

**The human provides:**
- Creative vision (premise, themes, tone)
- Direction (what should happen next)
- Feedback (what's working, what's not) — when they want to, not because they have to

**The AI provides:**
- Structure (outline, plot state, pacing)
- Content (prose, dialogue, description)
- Quality (grammar-constrained, coherent)
- Speed (fast generation, easy revision)

## Agent architecture: plan-and-execute (predetermined mode selection)

RoCo AI supports two agent execution patterns, both using **grammar-constrained
output everywhere** — never free-form JSON extraction:

### Pattern 1: Plan-first (deterministic)

```
System instruction + User message
  ↓ BNF-constrained LLM call → valid JSON plan
┌──────────────────────────────────────┐
│ Classic Rust loop over steps         │
│ for step in plan.topological_order() │
│   result = tool_dispatch(step)       │
│   or model_subtask(step)             │
│   eval_verify(step, result)          │
│   if !verified: inject_subtasks()    │
│ end                                  │
└──────────────────────────────────────┘
  ↓
Final assembled output
```

Key properties:
- **No free-form intermediaries**: Every LLM call produces BNF-valid output;
  `serde_json::from_str()` always succeeds — no heuristics, no brace-counting
- **Classic code owns control flow**: iteration order, dependencies, termination
  are all Rust logic; the model only fills content slots
- **Self-prompting chain**: Each completed step feeds its result as context for
  the next query via an auto-assembled prompt template
- **Configurable mechanistic depth**: Shallow (execute as-is), Medium (verify each
  step against evals), Deep (auto-inject subtasks on rejection), Autonomous
  (self-prompting chain runs until all evals pass)

### Pattern 2: ReAct (open-ended)

The existing `Agent::run()` loop — model-driven iteration where the model decides
how many steps it needs. Still grammar-constrained, but the loop structure is
probabilistic (model emits final_answer to stop).

Both patterns use the same grammar infrastructure (`BnfConstraint` + vocab trie)
The difference is whether the **iteration count** is predetermined or model-driven.

## Status

- **Inference**: works end-to-end on `RWKV-7 g1h 2.9B` (FP16 PTH → converted
  to SafeTensors → quantized to NF4 at runtime on RTX 2050 / AMD iGPU).
- **Grammar-constrained decoding**: **`BnfConstraint`** wraps the
  token-level BNF engine provided by `crates/bnf-engine`, which itself
  wraps `kbnf 0.5` (with `ahash`-backed vocabulary). The reason it's
  isolated in its own crate: `kbnf`'s generic types (specifically
  `string-interner`'s recursive `StringInterner`) trip Rust's
  `error[E0275]` (type recursion overflow) when they enter the same
  compilation unit as `web-rwkv`'s `TokioRuntime`; isolating it keeps
  `inference` clean. `BnfConstraint` is still built on top in
  `crates/grammar/src/bnf.rs` (vocab-built, `accept_token` /
  `apply_mask` API). The `Schema` builder lives in
  `crates/grammar/src/schema.rs` (`object().prop(...).build().to_gbnf()`),
  with the JSON-Schema→GBNF converter in
  `crates/grammar/src/json_schema.rs`. The old `bnf_sampler`+`qp-trie`
  path and the `schoolmarm` fallback are no longer in the build; the
  current rewrite happened in commit `22ebe66`.
- **State-mixing / multi-session**: **Phase 1 implemented.**
  `CompletionRequest::session` → session-based state save/restore via
  `AnyState::back()`/`load()`, with an LRU pool (`max_sessions = 8`) in
  `crates/session`. Enables persistent conversations across calls. Phase 2
  (N-slot GPU pool with concurrent batching) and Phase 3 (tensor blending)
  are forward work. `RwkvBackend::save_state()`/`load_state()` now serialize the
  recurrent vector (incl. per-head min-decay channels) to/from bytes — usable
  for introspection (see `docs/imagined-usecases.md`).
- **Plan-and-execute harness**: **Implemented.** `Planner::plan()` produces
  grammar-constrained plans; `Plan::execute()` runs wave-level dependency-aware
  execution with topological sorting. Self-prompting chain assembly and inline
  eval verification are documented in `goals/` but not yet wired into production.
- **Mechanistic agent**: **Implemented.** `MechanisticAgent` (`crates/agent/src/mecha_agent.rs`) provides a code-driven controller + router pattern: model only fires at fixed, grammar-constrained points (`classify` → `think_with_intent` → `repair_derive` → dispatch). Routes register `(type, domain)` handlers that write into a sandboxed workspace. Supports repair loops with temperature/tokens decay, context budget gating via `ContextManager`, and self-correction chains.
- **Story generation engine**: **Implemented end-to-end.** Dynamic
  outline expansion, plot state tracking, context assembly, chapter
  continuation, quality evaluation (model-as-judge, 7 dimensions),
  revision support, and session persistence — all in
  `crates/agent/src/story_engine.rs` and friends. The interaction layer
  (outline editing, NL feedback, real-time preview, story direction,
  chapter steering, writing assistant, commentary, interaction modes)
  is also implemented; the surface that ties these into the live CLI
  is `crates/cli/examples/story_human.rs` (the canonical entry point for
  human-AI writing sessions).
- **Observability**: **Implemented.** `ObservabilitySystem` records all model calls, decisions, actions, and quality assessments. Enables debugging, interpretability, and auditing.
- **Reversibility**: **Implemented.** `VersionControl` provides workspace snapshots, action history, undo/redo, and rollback. Every agent action is reversible.
- **Commentary**: **Implemented.** `Commentary` system provides bidirectional commentary — agent-generated explanations for every artifact, plus human annotations, verdicts, and notes. Every artifact can be reviewed and annotated by both agent and human.
- **Writing Assistant**: **Implemented.** `WritingAssistant` analyzes user input, provides continuation suggestions, fill-in-the-middle, diff analysis, cross-referencing, and tagging.
- **Interaction Modes**: **Implemented.** `InteractionMode` lets the human choose: interactive (see each chapter) or automatic (run to completion). Human can switch modes at any time.
- **Natural Language Feedback**: **Implemented.** `FeedbackParser` parses human feedback into structured directives. Quick parse for simple commands, model-based parsing for complex feedback.
- **Outline Editing**: **Implemented.** `OutlineEditor` for collaborative outline creation and modification. Commands: add, remove, move, modify, regenerate.
- **Story Direction**: **Implemented.** `StoryDirection` captures and applies human's creative vision throughout generation.
- **Chapter Steering**: **Implemented.** `ChapterSteerer` for pause/resume/steer mid-generation.
- **Pull-based context management**: **Implemented.** `ContextManager` (`crates/agent/src/context.rs`) pulls relevant snippets from session store, memory store, and workspace files; scores via Jaccard word overlap; gates inclusion by token budget before each inference call.
- **ReAct loop**: **Implemented.** `Agent::run()` in `crates/agent/src/agent.rs`
  with `think` blocks, tool dispatch, gradual tool disclosure, and budget limits.
- **Chat CLI**: `roco chat` example (`crates/cli/examples/chat.rs`) provides
  a terminal REPL with streaming output, session persistence, grammar
  constraints, and Ctrl+C interrupt. The `agent` example
  (`crates/cli/examples/agent.rs`) runs the ReAct loop.
- **Story human workflow**: `crates/cli/examples/story_human.rs` is the
  canonical entry point for end-to-end story generation with human-AI
  collaboration. Other story examples (`story_collaborative`,
  `story_engine`, `story_full`, `story_pilot`, `story_eval`,
  `story_step_eval`) exercise narrower slices — pilots, pure evals, full
  pipeline with all bells on, etc.
- **bnf-engine**: a dedicated isolation crate (`crates/bnf-engine/`)
  wraps `kbnf 0.5`. The reason it's its own crate is documented above
  (avoids the `string-interner` recursion E0275 against `web-rwkv`'s
  `TokioRuntime`).
- **Model loading**: `crates/inference/src/backend.rs` auto-detects
  model shape from `Loader::info`, picks a quantization plan from
  on-disk file size, and resolves model paths from
  `$RWKV_MODEL` / `models/*.st`.
- **Cleanup segfault**: `free(): invalid size` at process exit — **fixed**.
  wgpu/tokio resources now drop in-order on the dedicated actor thread
  via `RwkvBackend::Drop`.

## Layout

```
roco_ai/
├── Cargo.toml              # workspace: 14 crates
├── crates/
│   ├── engine/             # roco_engine — ModelBackend trait, MockBackend, eval suite
│   ├── bnf-engine/         # roco_bnf_engine — kbnf 0.5 isolation crate (E0275 workaround)
│   ├── grammar/            # roco_grammar — BnfConstraint, Schema, schema_to_gbnf
│   ├── inference/          # roco_inference — RwkvBackend, RwkvActor, quant proxy
│   ├── message/            # roco_message — roles, format, gbnf, retry/error
│   ├── session/            # roco_session — LruSessionPool
│   ├── tools/              # roco_tools — Tool trait, ToolRegistry, builtins, parse
│   ├── workspace/          # roco_workspace — Workspace (sandbox boundary)
│   ├── agent/              # roco_agent — ReAct loop, mechanistic controller, story engine
│   │   ├── story_engine.rs      # Dynamic story generation
│   │   ├── story_direction.rs   # Creative vision capture + application
│   │   ├── outline_editing.rs   # Outline add/remove/move/modify
│   │   ├── chapter_steering.rs  # Mid-generation pause/steer/resume
│   │   ├── natural_feedback.rs  # NL feedback → structured directives
│   │   ├── quality.rs           # Quality metrics and critique
│   │   ├── evals.rs             # Model-as-judge evaluation
│   │   ├── story_persistence.rs # Save/load story state
│   │   ├── observability.rs     # Traces, logs, audit trail
│   │   ├── reversibility.rs     # Undo/redo, version control
│   │   ├── commentary.rs        # Bidirectional agent/human commentary
│   │   ├── writing_assistant.rs # Continuation, fill-middle, analysis, diff
│   │   ├── interaction.rs       # Interactive / automatic modes
│   │   ├── agent_chat.rs        # Folder-bound session loop
│   │   └── mecha_agent.rs       # Mechanistic controller + router
│   ├── chat-common/        # roco_chat_common — Conversation, DisplaySettings
│   ├── cli/                # roco_cli — `roco` bin + examples
│   │   └── examples/
│   │       ├── story_human.rs       # ★ canonical human-AI story workflow
│   │       ├── story_collaborative.rs # earlier conversational variant
│   │       ├── story_engine.rs      # dynamic story engine (no UI)
│   │       ├── story_full.rs        # full settings demo
│   │       ├── story_pilot.rs       # grammar-constraint pilot
│   │       ├── story.rs             # minimal 3-chapter pipeline
│   │       ├── story_eval.rs        # story eval harness
│   │       ├── story_step_eval.rs   # per-step eval tracking
│   │       ├── chat.rs              # terminal REPL
│   │       ├── agent.rs             # ReAct loop with tools
│   │       ├── agent_chat.rs        # agent_chat session demo
│   │       ├── eval_suite.rs        # suites the `roco eval` subcommand runs
│   │       ├── grammar_smoke.rs     # grammar-constrained decode smoke test
│   │       ├── state_mix_alphas.rs  # state-mixing experiments
│   │       ├── state_mix_eval.rs    # state-mix eval cases
│   │       ├── state_tune_eval.rs   # state-tune eval cases
│   │       └── strategy_comparison.rs / task_state_tune_eval.rs # tuners
│   ├── tui/                # roco_tui — story pane, plot state viewer, shortcuts
│   ├── server/             # roco_server — HTTP + story routes
│   └── gateway/            # roco_gateway — API gateway
├── vendor/web-rwkv/        # patched web-rwkv dependency ([patch.crates-io] in Cargo.toml)
├── apps/                   # web frontends (chat, studio, editor) and editor plugins (vscode, zed)
├── models/                 # RWKV .st files; on-disk truth for model resolution (gitignored)
├── assets/vocab/           # rwkv_vocab_v20230424.json (the tokenizer)
├── scripts/                # pth_to_st/ and gguf_to_st/ model converters
├── GBNF/                   # hand-written kbnf-dialect grammars for story prose handlers
├── templates/              # prompt templates used by the story engine
├── memory/                 # agent memory store scratchpads
├── datasets/               # in-tree training/eval datasets (plot_overview, project_planning, …)
├── docs/                   # long-form human docs (separate from goals/)
├── agents/                 # agent run artifacts / scratch
├── goals/                  # product roadmap (see goals/index.md)
│   ├── story-engine/       # Story engine roadmap (human-AI interaction focus)
│   ├── agent/, agent_chat/, browser_use/, coder/, infer/, message/,
│   │   mechanistic-agent/, testing/, workspace/  # prerequisite layers
│   └── future/             # archived goals (FAISS, dreaming, UIs, etc.)
├── evals/results/          # rwkv benchmark JSON outputs
├── devenv.{yaml,nix}       # Nix dev shell (rust + Vulkan)
├── Makefile                # rwkv-focused dev targets
└── .env                    # local API keys (gitignored)
```

### What each crate holds

| Crate | Key modules | Responsibility |
|---|---|---|
| `engine` | `backend.rs`, `eval.rs`, `cases.rs`, `types.rs` | `ModelBackend` trait, `MockBackend`, eval harness + cases |
| `bnf-engine` | `lib.rs` | Isolated `kbnf 0.5` wrapper exposing `BnfMask`-compat API; separate crate to avoid E0275 vs `web-rwkv` |
| `grammar` | `bnf.rs`, `schema.rs`, `strategies.rs`, `json_schema.rs`, `kbnf_compat.rs`, `grammar_library.rs` | `BnfConstraint` (over `bnf-engine`), `Schema` builder, JSON-Schema→GBNF, `StoryGrammar` registry |
| `inference` | `backend.rs`, `actor.rs`, `sampling.rs`, `quant.rs`, `config.rs` | `RwkvBackend`, `RwkvActor` thread, sampling, quant proxy |
| `message` | `format.rs`, `roles.rs`, `gbnf.rs`, `error.rs` | Role prefixes, prompt formatting, message GBNF, retry/error recovery |
| `session` | `pool.rs`, `store.rs`, `types.rs`, `error.rs` | `LruSessionPool`, session transcript stores, session types |
| `tools` | `tool.rs`, `registry.rs`, `builtins.rs`, `parse.rs` | `Tool` trait, `ToolRegistry`, 6 built-ins, tool-call parsing |
| `workspace` | `workspace.rs` | `Workspace` sandbox boundary |
| `agent` | `story_engine.rs`, `quality.rs`, `evals.rs`, `story_persistence.rs`, `mecha_agent.rs`, `context.rs`, `observability.rs`, `reversibility.rs`, `commentary.rs`, `writing_assistant.rs`, `interaction.rs`, `natural_feedback.rs`, `outline_editing.rs`, `story_direction.rs`, `chapter_steering.rs`, `agent_chat.rs`, etc. | Story engine, quality metrics, evaluation, persistence, ReAct loop, mechanistic controller, all human-AI interaction surfaces |
| `chat-common` | `conversation.rs`, `display.rs` | `Conversation`, `DisplaySettings` (shared across frontends) |
| `cli` | `bin/roco.rs` + `examples/` | `roco` binary, story examples |
| `tui` | `app.rs`, `widgets/` | Story pane, plot state viewer, keyboard shortcuts |
| `server` | `lib.rs`, `routes.rs`, `story_routes.rs` | HTTP server with story routes |
| `gateway` | `lib.rs` | API gateway |

## Goals

`goals/` is the product roadmap, organized as prerequisite-ordered layers
from the local RWKV-7 engine up to a collaborative story writing tool:

| Layer | What it covers | State |
|---|---|---|
| `infer/` | inference engine (model, quant, state, decoding, structured output) | ✅ complete |
| `message/` | chat protocol (instructions, formatting, tool calls, chat CLI) | ✅ core done |
| `workspace/` | the environment the agent acts in | ✅ sandbox + scoped tools |
| `agent/` | the autonomous agent loop and its capabilities | ✅ core loop done |
| `mechanistic-agent/` | code-driven controller + router | ✅ implemented (grammar gap in story prose remains) |
| `story-engine/` | collaborative story writing engine (outline → wiki → chapter → publish) | ✅ end-to-end (prose-BNF coverage still in progress) |
| `agent_chat/` | persistent workspace or folder-bound agent sessions | ✅ working (`crates/cli/examples/agent_chat.rs`) |
| `browser_use/` | driving a real browser | ⬜ not started |
| `testing/` | eval harness, oracles, regression gates | ✅ done |
| `coder/` | **(future)** the agent's own develop/test/lint loop | ⬜ not started |

Each folder contains an `index.md` listing its goals in dependency order.

There is also a **`future/`** tree — archived goals that amplify a working core.

## Quickstart

```bash
cargo run --bin roco -- eval           # run evals, snapshot saved
cargo run --bin roco -- bless          # bless current snapshot as new oracle
cargo run --bin roco -- rwkv           # smoke-test the RWKV backend
cargo run --bin roco -- grammar        # grammar-constrained decode smoke test
cargo run --bin roco -- gpu-check      # show Vulkan device + model status
cargo build --release                  # all crates (release for GPU work)
```

### Story generation

```bash
# Canonical entry point: human-AI collaborative story writing
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli \
  "Write a xianxia story about a lone cultivator"

# Earlier conversational variant (slightly different UX)
RWKV_MODEL=... cargo run --release --example story_collaborative -p roco-cli \
  --interactive "Write a dark fantasy"

# Full settings demo (interactive + unlimited + quality threshold)
RWKV_MODEL=... cargo run --release --example story_full -p roco-cli \
  --interactive --unlimited "Write an epic fantasy"

# Grammar-constrained pilot pipeline (proves every stage is BNF-bounded)
RWKV_MODEL=... cargo run --release --example story_pilot -p roco-cli \
  "Write a heist story"
```

### Testing convention

Run tests directly with `cargo test`. No shell redirects, no temp files.
If a test fails, the exit code will be non-zero and the failure messages
will appear in the terminal output — inspect them directly.

```bash
# Single crate
cargo test -p roco-agent

# Full workspace
cargo test --workspace

# Workspace-only compile check (fast)
cargo check --workspace

# Linting
cargo clippy --workspace --all-targets -- --deny warnings
```

When debugging a flaky or hanging test, add `--nocapture` to see print!
output and `-q` to reduce noise:

```bash
cargo test -p roco-agent -- agent_chat::tests:: --nocapture -q
```

Rules:
- **Never** redirect test output to files with `>` or `2>&1`.
- If output needs capturing for later inspection, use `script` (terminal recorder)
  or `tee` — but prefer reading terminal output directly.
- Fix test failures instead of hiding them behind redirection.

> **The execution environment is always inside `devenv shell`.** The `roco`
> command is defined as a devenv script in `devenv.nix` (`scripts.*.exec`) and
> maps to the corresponding `cargo run -p … --example …` invocation. It is
> always available — you can also run the binary directly via
> `cargo run --bin roco -- `. The model is auto-detected from
> `models/*.st` (symlinked).
>
> **Features are enabled by default.** The `grammar` feature (on
> `inference` / `message`) wires in BNF-constrained decoding. All functionality
> is available without `--features`.
>
> **Snapshot/bless workflow:** Every `roco eval` saves a `.snapshot.json`
> next to the report. When the output is acceptable, run `roco bless` to
> update the source `oracle:` fields, making the current output the new
> pass/fail reference.

| Variable | Effect | Default |
|---|---|---|
| `RWKV_MODEL` | Absolute path to a `.st` SafeTensors file | First `rwkv7-*.st` in `models/` or `../models/` |
| `RWKV_VOCAB` | Path to vocab JSON | First matching `rwkv_vocab_v20230424.json` next to `RWKV_MODEL` |
| `RWKV_QUANT` | Override auto-quant: `none`, `nf4=N`, or `N` (Int8 N layers) | Auto-picked (NF4 if file ≥ 1.5 GB and GPU has coop matrix; else Int8; else no-quant if file < 1.5 GB) |
| `RWKV_ADAPTER` | Substring match against GPU adapter name | First Vulkan adapter with coop-matrix |
| `RWKV_GRAMMAR` | GBNF grammar to constrain decoding | unset |
| `RWKV_PIPELINE_CACHE_DIR` | Override the WGPU pipeline cache directory | `/tmp/roco-pipeline-cache` |
| `RWKV_QUANT_CACHE_DIR` | Override the quantized-weight cache directory | `/tmp/roco-quant-cache` |
| `RWKV_CHUNK` | Tokens processed in a single `frontend::infer` call (chunking trades throughput vs prompt buffering) | `128` |

## Build with `--release` for GPU work

`build_v7()` hangs in **debug** builds on most consumer GPUs — wgpu
validation layers, slow unoptimized shader compilation, and GPU-driver
TDR interact to lose the context. The harness always builds in release.
Release builds complete the load in ~18-25 s and generate ~16-20 tok/s
on RTX 2050 / NF4 / 2.9B.

If a debug build hangs regardless: try `RWKV_ADAPTER=llvmpipe` for the
CPU fallback (slow but reliable) or `RWKV_QUANT=8` to force Int8.

## Lessons Learned

### The Grammar-First Principle
**Every model call must go through a BNF grammar.** Free-form prompting on small RWKV models
(1B–2.9B) produces meta-commentary contamination that no amount of system prompting, temperature
decay, or post-processing can reliably eliminate. When output must satisfy a grammar, the sampler
rejects non-conforming tokens at every step — contamination literally cannot occur.

### The `<think>>` Tag Problem
Undertrained base RWKV models consistently leak planning text into output:
- System prompts saying "no thinking" have zero effect — and **backfire**: a
  system instruction like "never use `<think>` tags" primes the model to emit
  `<think>` (verified in `crates/cli/examples/prompt_probe_eval.rs`: the
  "no-think instruction" probe emitted `<think>` on both runs).
- Temperature decay has minimal impact — the behavior persists across all settings
- A **bare `Assistant:` start defaults to an open `<think>` block** (the root
  cause of contamination in the story pipeline). Probing an empty context with
  `prefill = None` produced `<think>` as the first tokens.
- Post-processing stripping is fragile because models often never close their think tags
- Pre-filling a **CLOSED** think block (`<think></think>`) before the prompt puts
  the model straight into content mode and it does **not** re-open `<think>`
  (`NO_THINK_PREFILL` in `crates/engine/src/backend.rs`). This is the reliable
  suppression lever — much better than banning `<`/`>` at the grammar level,
  which also blocks legitimate prose and can starve the sampler.

### Think-tag state-tuning (experimentally validated)
The model's think-tag prior was probed directly (`prompt_probe_eval.rs`) by
feeding the training-prompt prefixes as `prefill` after `Assistant:`:
- `Assistant: <think` → model closes the tag (`>`) and emits chain-of-thought.
- `Assistant: <think></think>` → model emits content, no re-open. **Reliable.**
- `Assistant: <reason>…</reason>` → model emits a `<plan>` outline instead of
  `<think>`. There are **alternate planning markers** (`<reason>`, `<plan>`) —
  these are the "certain areas" where thinking is acceptable.
- Baking a no-think session (`bake_no_think_session`) biases the recurrent state
  away from `<think>`, but it is a *soft* bias and noisier than the prefill;
  the correct-role bake still showed occasional `User:`-turn leakage. Prefer the
  closed-think **prefill** for deterministic suppression.

**Design:** suppress `<think>` by (a) prefilling `<think></think>` (or a content
lead-in like "Sure! Here is the chapter:") whenever an assistant turn starts,
and (b) generating free prose **outside** the JSON envelope via the per-handler
BNF grammars in `GBNF/` with that prefill. For the stages that benefit from
reasoning (outline expansion, plot-state extraction, quality critique), *intentionally*
prefill `<think>` to get the trace, then strip the `<think>…</think>` span before
parsing the JSON — so thinking is allowed only in those designated regions.
The grammar-ban approach (`<`/`>` forbidden) is deprecated in favor of this.

### Prompt-format & format-lock experiments (2026-07-18)
Probed alternative message formats, System-instruction limits, agentic
induction, and newline masking (`crates/cli/examples/prompt_format_probe_eval.rs`):
- **Format lock-in**: only the native `System:/User:/Assistant:` format is
  followed. ChatML / Alpaca / `Human:/Assistant:` are out-of-distribution and
  *trigger* `<think>` (the model falls back to its training prior). The
  `NO_THINK_PREFILL` still suppresses `<think>` across **all** formats — it is a
  token-level recurrent-state effect, format-independent. You cannot retrain
  the model onto a new format by prompting, but you can apply the same
  state-tune regardless of surface format.
- **System instructions are inert for think suppression**: none / neutral /
  "no think" / "think step by step" / contradictory *all* emitted `<think>` in
  the probe. Do not rely on system prompts to control think emission.
- **Agentic behavior is inducible by a simple prompt — but only with the
  no-think prefill**: the agentic system prompt + `NO_THINK_PREFILL` emitted
  `<action>plan_story_outline</action>`; without the prefill the model just
  thought and never emitted the action. Closing think is what lets the
  structured action surface.
- **Line-prefix newline masking does NOT work via prefill**: a `▸ `/`> ` prefill
  was dropped after the first token (0/3, 0/2 lines kept the prefix). To force
  per-line structure, a **grammar** mandating a line-prefix nonterminal is
  required, not a prefill.
- **Min-decay state-vector monitoring works**: `RwkvBackend::save_state()`
  serializes the recurrent vector (the per-head min-decay channels are the last
  two of `head_size+2`). Their norm (~145–157) and 256-bin entropy (~0.6–0.8
  bits) are cheap, computable signals that vary by prompt (e.g. a contradictory
  system prompt gave the highest entropy). See `docs/imagined-usecases.md`.

### Architecture Decisions Proven Correct
- Code owns control flow, LLM only fires at fixed grammar-bounded points
- Pull-based context injection over push-based bulk data transfer
- Jaccard word overlap relevance scoring sufficient for initial use
- Arc-owned context sources cleanly satisfy `'static` bounds
- Persistent timestamped workspaces prevent collision across repeated runs

### Human-AI Collaboration Principles
- The human is the author, the AI is the tool
- Every interaction should feel natural and intuitive
- Give control, not just output
- Respect the human's creative vision
- Make the human feel empowered, not replaced

### Interim Workarounds (Signaling Where Grammars Are Needed)
The story pipeline still uses pre-fill + strip-think-blocks as interim
measures in the **prose-only** handlers (the JSON envelope of every
stage is BNF-constrained via `crates/grammar/src/schema.rs`; the
*content* inside the prose envelope is what's free-form). These are
explicit signals that domain-specific BNF grammars should be added:
- outline handler → needs `outline.bnf`
- wiki handler → needs `wiki.bnf`
- chapter handlers → need `chapter_prose.bnf`
- validation handler → needs `validation_report.bnf`
- synopsis handler → needs `synopsis.bnf`

**Status (2026-07-18):** the per-handler grammars now **exist** — the
broken zero-byte `GBNF` placeholder has been replaced by a real
`GBNF/` directory containing `outline.bnf`, `wiki.bnf`,
`chapter_prose.bnf`, `validation_report.bnf`, and `synopsis.bnf`, all in
kbnf GBNF dialect. They are embedded and exposed by
`roco_grammar::grammar_library::StoryGrammar` (with `source()` / `kbnf()`
accessors) and validated against `roco-bnf-engine` in
`crates/grammar/src/grammar_library.rs` tests — every grammar loads in
the real engine and accepts a valid sample to completion.

**State-tune mechanism (experimentally validated, 2026-07-18):** rather
than banning `<`/`>` at the grammar level, suppress `<think>` by
prefilling `NO_THINK_PREFILL` (`<think></think>`, see
`crates/engine/src/backend.rs`) whenever an assistant turn starts — the
`prompt_probe_eval.rs` experiment confirmed this reliably yields content
with no re-opened think block, whereas a bare `Assistant:` start defaults
to `<think>` and a system "no-think" instruction *backfires*. The
remaining step is to route each prose handler through its grammar,
generating prose **outside** the JSON envelope via `StoryGrammar::kbnf()`
(which structurally excludes `<`/`>`, so `<think>` cannot appear); for the
JSON-envelope stages that must permit `<` (e.g. to capture a reasoning
trace), prefill `NO_THINK_PREFILL` or strip a leading `<think>…</think>`
span before parsing JSON. For the stages that benefit from reasoning
(outline expansion, plot-state extraction, quality critique),
intentionally prefill `<think>` to capture the trace, then strip the
`<think>…</think>` span before parsing JSON — so thinking is confined to
those designated regions.

The `crates/grammar/src/strategies.rs` module already exposes
`StrategySelector` so callers can pick a per-handler strategy; the
`RawGbnfStrategy` / `StoryGrammar` pair is the intended vehicle for
wiring the coverage in.

## Next Things

### Status snapshot — what's left

All Phase 2 human-AI interaction surfaces (collaborative outline editing,
natural-language feedback, real-time preview, easy revision with diff,
story direction persistence, chapter steering) are **implemented** in
`crates/agent/src/`. What remains is wiring them into the production
surface and tightening grammar coverage:

1. **Per-handler BNF grammars** — the domain grammars for the prose
   handlers (outline/wiki/chapter/validation/synopsis) now exist in
   `GBNF/` and are exposed via `roco_grammar::grammar_library` (validated
   against `roco-bnf-engine`). Remaining: route each prose handler
   through its grammar so prose is generated outside the JSON envelope,
   replacing the pre-fill + strip-think workaround.
2. **Grammar coverage audit** — enumerate every free-form
   `backend.complete()` call in the story pipeline; each one is a
   contamination risk on under-trained RWKV models and should be
   bounded by a `BnfConstraint`.
3. **Live eval continuity** — keep `cargo test -p roco-agent` and
   `roco eval` green as code lands; don't regress the 14/15b baseline
   on the g1h 2.9B model.
4. **Story human CLI polish** — `story_human.rs` is the canonical UX;
   fold bug reports from use into it.

### Infrastructure status (mostly resolved)

- ~~JSON-Schema → GBNF converter~~ ✅ done (`crates/grammar/src/json_schema.rs`)
- ~~Dead module cleanup~~ ✅ done
- ~~Cleanup segfault~~ ✅ fixed (commit on process exit path)
- ~~`bnf_sampler` integration~~ ✅ done (later **replaced** by `kbnf` in `bnf-engine`, commit `22ebe66`)
- ~~State pool Phase 1~~ ✅ done
- ~~Monorepo restructuring~~ ✅ done (14 crates now)
- ~~Plan-and-execute architecture documented~~ ✅ done
- ~~Mechanistic agent implementation~~ ✅ done
- ~~Story engine core~~ ✅ done
