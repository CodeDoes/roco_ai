# AGENTS.md — RoCo AI

> Operational manual for working in this repo.

## What this is

A Rust workspace whose only inference path is `crates/inference/src/backend.rs`
(RWKV-7 via `web-rwkv` + WGPU). The repo has been pared down to the
local-RWKV critical path and restructured into focused crates — the
`crates/inference` library plus `crates/grammar`, `crates/engine`, and the
supporting crates (`message`, `tools`, `session`, `workspace`, `agent`,
`chat-common`, `cli`, `tui`, `server`, `gateway`), the `vendor/web-rwkv`
patch, the `scripts/` model converters, and the `assets/vocab` tokenizer.
Everything non-RWKV (orchestrator crates, gateway/web frontends, Docker,
agent/eval scaffolding) has been removed; git history preserves it.

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
- **Grammar-constrained decoding**: **`BnfConstraint`** (`bnf_sampler`
  v0.3.8 + `qp-trie` vocabulary + GBNF→BNF converter) is the primary
  engine in `crates/grammar/src/bnf.rs`. Falls back to schoolmarm
  automatically when the GBNF uses features `bnf_sampler` can't parse
  (character classes `[...]`, quantifiers `*`). JSON-Schema → GBNF converter
  is done (`crates/grammar/src/json_schema.rs`) with object/array support.
- **State-mixing / multi-session**: **Phase 1 implemented.**
  `CompletionRequest::session` → session-based state save/restore via
  `AnyState::back()`/`load()`, with an LRU pool (`max_sessions = 8`) in
  `crates/session`. Enables persistent conversations across calls. Phase 2
  (N-slot GPU pool with concurrent batching) and Phase 3 (tensor blending)
  are forward work.
- **Plan-and-execute harness**: **Implemented.** `Planner::plan()` produces
  grammar-constrained plans; `Plan::execute()` runs wave-level dependency-aware
  execution with topological sorting. Self-prompting chain assembly and inline
  eval verification are documented in `goals/` but not yet wired into production.
- **Mechanistic agent**: **Implemented.** `MechanisticAgent` (`crates/agent/src/mecha_agent.rs`) provides a code-driven controller + router pattern: model only fires at fixed, grammar-constrained points (`classify` → `think_with_intent` → `repair_derive` → dispatch). Routes register `(type, domain)` handlers that write into a sandboxed workspace. Supports repair loops with temperature/tokens decay, context budget gating via `ContextManager`, and self-correction chains.
- **Story generation engine**: **Implemented (core).** Dynamic outline expansion, plot state tracking, context assembly, chapter continuation, quality evaluation, revision support, session persistence. **Human-AI interaction in progress.**
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
├── Cargo.toml              # workspace: 13 crates
├── crates/
│   ├── engine/             # roco_engine — ModelBackend trait, MockBackend, eval suite
│   ├── grammar/            # roco_grammar — BnfConstraint, schema_to_gbnf
│   ├── inference/          # roco_inference — RwkvBackend, RwkvActor, quant proxy
│   ├── message/            # roco_message — roles, format, gbnf, retry/error
│   ├── session/            # roco_session — LruSessionPool
│   ├── tools/              # roco_tools — Tool trait, ToolRegistry, builtins, parse
│   ├── workspace/          # roco_workspace — Workspace (sandbox boundary)
│   ├── agent/              # roco_agent — ReAct loop, mechanistic controller, story engine
│   │   ├── story_engine.rs      # Dynamic story generation
│   │   ├── quality.rs           # Quality metrics and critique
│   │   ├── evals.rs             # Model-as-judge evaluation
│   │   ├── story_persistence.rs # Save/load story state
│   │   ├── observability.rs     # Traces, logs, audit trail
│   │   └── reversibility.rs     # Undo/redo, version control
│   ├── chat-common/        # roco_chat_common — Conversation, DisplaySettings
│   ├── cli/                # roco_cli — `roco` bin + examples
│   │   └── examples/
│   │       ├── story.rs           # Basic story pipeline (3 chapters)
│   │       ├── story_engine.rs    # Dynamic story engine
│   │       └── story_full.rs      # Full example with all features
│   ├── tui/                # roco_tui — terminal UI (stub)
│   ├── server/             # roco_server — HTTP server (stub)
│   └── gateway/            # roco_gateway — API gateway (stub)
├── vendor/web-rwkv/        # patched web-rwkv dependency ([patch.crates-io] in Cargo.toml)
├── models/                 # RWKV .st files; on-disk truth for model resolution (gitignored)
├── assets/vocab/           # rwkv_vocab_v20230424.json (the tokenizer)
├── scripts/                # pth_to_st/ and gguf_to_st/ model converters
├── goals/                  # product roadmap (see goals/index.md)
│   ├── story-engine/       # Story engine roadmap (human-AI interaction focus)
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
| `grammar` | `bnf.rs`, `json_schema.rs` | `BnfConstraint` (bnf_sampler + vocab), JSON-Schema→GBNF |
| `inference` | `backend.rs`, `actor.rs`, `sampling.rs`, `quant.rs`, `config.rs` | `RwkvBackend`, `RwkvActor` thread, sampling, quant proxy |
| `message` | `format.rs`, `roles.rs`, `gbnf.rs`, `error.rs` | Role prefixes, prompt formatting, message GBNF, retry/error recovery |
| `session` | `pool.rs`, `store.rs`, `types.rs`, `error.rs` | `LruSessionPool`, session transcript stores, session types |
| `tools` | `tool.rs`, `registry.rs`, `builtins.rs`, `parse.rs` | `Tool` trait, `ToolRegistry`, 6 built-ins, tool-call parsing |
| `workspace` | `workspace.rs` | `Workspace` sandbox boundary |
| `agent` | `story_engine.rs`, `quality.rs`, `evals.rs`, `story_persistence.rs`, `mecha_agent.rs`, `context.rs`, etc. | Story engine, quality metrics, evaluation, persistence, ReAct loop, mechanistic controller |
| `chat-common` | `conversation.rs`, `display.rs` | `Conversation`, `DisplaySettings` (shared across frontends) |
| `cli` | `bin/roco.rs` + `examples/` | `roco` binary, story examples |
| `tui` | `app.rs`, `widgets/` | Terminal UI (stub) |
| `server` | `server.rs`, `routes.rs` | HTTP server (stub) |
| `gateway` | `gateway.rs`, `router.rs` | API gateway (stub) |

## Goals

`goals/` is the product roadmap, organized as prerequisite-ordered layers
from the local RWKV-7 engine up to a collaborative story writing tool:

| Layer | What it covers | State |
|---|---|---|
| `infer/` | inference engine (model, quant, state, decoding, structured output) | ✅ complete |
| `message/` | chat protocol (instructions, formatting, tool calls, chat CLI) | ✅ core done |
| `workspace/` | the environment the agent acts in | ✅ sandbox + scoped tools |
| `agent/` | the autonomous agent loop and its capabilities | ✅ core loop done |
| `mechanistic-agent/` | code-driven controller + router | 🟡 grammar gap remains |
| **story-engine/** | **collaborative story writing engine** | ✅ core done, 🟡 human-AI interaction in progress |
| `agent_chat/` | persistent workspace or folder-bound agent sessions | ✅ working |
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
# Basic story generation (3 chapters)
RWKV_MODEL=... cargo run --release --example story_engine -p roco-cli \
  "Write a xianxia story about a lone cultivator"

# Interactive mode (human-in-the-loop)
RWKV_MODEL=... cargo run --release --example story_engine -p roco-cli \
  --interactive "Write a dark fantasy"

# Full example with all features
RWKV_MODEL=... cargo run --release --example story_full -p roco-cli \
  --interactive --unlimited "Write an epic fantasy"
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
- System prompts saying "no thinking" have zero effect
- Temperature decay has minimal impact — the behavior persists across all settings
- Every stage gets contaminated unless blocked by grammar constraints
- Post-processing stripping is fragile because models often never close their think tags
- Pre-filling `<think>>...content...` before the prompt helps but doesn't solve root cause

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
The story pipeline currently uses pre-fill + strip-think-blocks as interim measures.
These patterns are explicit signals that domain-specific BNF grammars should be added:
- outline handler → needs `outline.bnf`
- wiki handler → needs `wiki.bnf`
- chapter handlers → need `chapter_prose.bnf`
- validation handler → needs `validation_report.bnf`
- synopsis handler → needs `synopsis.bnf`

## Next Things

### Human-AI Interaction (Current Focus)

1. **Collaborative outline editing** — human and AI co-create the outline
2. **Natural language feedback** — human gives feedback in plain English
3. **Real-time preview** — show chapter being generated
4. **Easy revision with diff** — show what changed
5. **Story direction persistence** — capture and respect human's creative vision
6. **Chapter steering** — pause and redirect mid-generation

### Infrastructure (Forward Work)

7. ~~JSON-Schema → GBNF converter~~ ~~**Done.**~~
8. ~~Dead module cleanup~~ ~~**Done.**~~
9. ~~Cleanup segfault~~ ~~**Fixed.**~~
10. ~~`bnf_sampler` integration~~ ~~**Done.**~~
11. ~~State pool Phase 1~~ ~~**Done.**~~
12. ~~Monorepo restructuring~~ ~~**Done.**~~
13. ~~Plan-and-execute architecture documented~~ ~~**Done.**~~
14. ~~Mechanistic agent implementation~~ ~~**Done.**~~
15. ~~Story engine core~~ ~~**Done.**~~

### Active priorities
1. **Collaborative outline editing** — human can say "add a chapter about X"
2. **Natural language feedback** — parse "make it darker" into directives
3. **Real-time preview** — stream chapter content to terminal
4. **Easy revision** — show diff between original and revised
5. **Story direction** — capture tone/style/themes at start
6. **Chapter steering** — pause generation, accept direction, resume
7. **Per-handler BNF grammars** — wire `BnfConstraint` into every story pipeline stage
8. **Grammar coverage audit** — identify all free-form `backend.complete()` calls
