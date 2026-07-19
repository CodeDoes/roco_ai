# AGENTS.md — RoCo AI

> Operational manual for working in this repo.
> **Read `roadmap/README.md` first** — it is the living plan and the
> definition of done. This file tells you *how to work*; `roadmap/` tells
> you *what to build* and *whether it's finished*.

## What this is

A Rust workspace whose only inference path is `crates/inference/src/backend.rs`
(RWKV-7 via `web-rwkv` + WGPU). The repo has been pared down to the
local-RWKV critical path and restructured into focused crates. The engine
layer is **done and frozen** — see Status below. Active work is the
**human-facing experience** (frontend), not the engine.

## Primary Goal

**A collaborative story writing tool where humans and AI work together to create stories.**

The human is the author. The AI is the tool. Every feature should amplify human creativity, not replace it.

The current plan lives in `roadmap/`. The old `goals/` scratchpad and
`PROGRESS.md` were removed on 2026-07-19 because they steered work toward
engine completeness (a feature marked ✅ when it merely *existed in a crate*)
instead of toward a usable, tested human experience.

## How to Work (attitude & behaviour)

These rules override any impulse to "just implement the next engine thing."

1. **The engine is frozen. Do not churn it.** `crates/inference`, `engine`,
   `grammar`, `bnf-engine`, `agent`, `session`, `message`, `tools`,
   `workspace` are correct and tested. Touch them only to fix a bug that
   blocks a frontend feature, and keep the change minimal.
2. **Build the experience, not the example.** A feature is not done because
   a Rust module or an `examples/*.rs` binary exists. It is done only when a
   human can reach it through the real UI and drive it. See Definition of
   Done in `roadmap/README.md`.
3. **Surface control, always.** Every artifact the AI produces is a
   *suggestion* until the human accepts it. Expose accept / modify / skip /
   stop visibly. Never hide pace control behind a menu.
4. **Tests are part of the feature.** If you add a surface, add a test that
   proves a human can drive it (unit for logic, integration/UI for the
   surface). No test = not done. Do not report a task complete without one.
5. **Small, reviewable steps.** Prefer one focused change over a large
   rewrite. Keep the build green (`cargo test --workspace`, `cargo clippy
   --workspace --all-targets -- --deny warnings`).
6. **Write progress where the human can see it.** After a meaningful
   change, append a line to `roadmap/progress.md` (what, where, done-or-not).
   Do not rely on git history to communicate trajectory.
7. **Be honest about partial work.** If you cannot meet the Definition of
   Done, say so. Do not mark a ✅ you have not earned.
8. **Don't gold-plate the core.** Grammar-coverage tidy-ups, extra
   example binaries, and new crate scaffolding are not progress unless they
   change what the human experiences. Resist them.
9. **Commit atomically.** After each meaningful, complete change (tests
   pass, builds green), create a git commit with a descriptive message.
   Do not batch unrelated changes. This keeps history bisectable and
   progress visible.

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

## Status — engine is frozen, experience is the work

**The Rust core is done, correct, and tested. Do not churn it.** The items
below are a compact record of what exists in the engine, kept so you know
what you can *build on* — not a to-do list. If you find yourself "finishing"
one of these, stop: it was already finished at the module level; the open work
is surfacing it in the UI (see `roadmap/ux.md`).

### Frozen core (build on, don't modify)
- **Inference** — `RwkvBackend` on a dedicated actor thread, RWKV-7 g1h 2.9B,
  NF4/Int8 quant, end-to-end.
- **Grammar-constrained decoding** — `BnfConstraint` over `bnf-engine` (kbnf
  0.5, isolated crate to avoid the `E0275` recursion against `web-rwkv`).
- **State save/load + multi-session** — `RwkvBackend::save_state/load_state`,
  LRU session pool (`max_sessions = 8`).
- **Story engine** — outline expansion, plot-state tracking, context
  assembly, chapter continuation, quality eval (7 dims), revision,
  persistence — all in `crates/agent/src/`.
- **Human-control logic (tested)** — `interaction.rs` (pace modes),
  `story_direction.rs`, `chapter_steering.rs`, `commentary.rs`,
  `natural_feedback.rs`, `outline_editing.rs`, `writing_assistant.rs`,
  `reversibility.rs` (VersionControl). These encode accept/skip/stop/pause.
- **Per-handler GBNF grammars** — `GBNF/` + `StoryGrammar` registry; prose
  handlers still use prefill+strip-think as interim coverage.
- **Tooling** — `roco eval` / `bless` snapshot workflow, `roco chat`,
  `story_human.rs` (CLI surface), Zed LSP.

### What is actually missing (the real work)
This is the gap, and it is *experience*, not engine:
1. **No tested human surface.** The control logic above is not exposed in any
   UI with tests. `apps/` webapps have zero tests and don't surface
   accept/skip/stop/pause/commentary.
2. **Frontend migration.** Moving off the untested webapps toward a gpui
   desktop app so the UI lives in the same tested Rust tree as the engine
   (see `roadmap/blocked.md`).
3. **Per-feature UI + tests** for each flow in `roadmap/ux.md`.

If a task isn't on this short list or in `roadmap/`, question whether it
moves the human experience forward before starting it.

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
├── docs/                   # long-form human docs (separate from roadmap/)
├── agents/                 # agent run artifacts / scratch
├── roadmap/                # LIVING PLAN — README.md (definition of done), ux.md,
│                           #   progress.md (append-only), blocked.md (parking lot)
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

## Plan & progress

The product plan and the definition of done live in **`roadmap/`**
(`README.md`, `ux.md`, `progress.md`, `blocked.md`). This replaced the old
`goals/` scratchpad, which steered work toward engine completeness rather
than the human experience. Read `roadmap/README.md` before starting any task.

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

The detailed plan is in `roadmap/`. In priority order, the work is about the
**human experience**, not the engine:

1. **Frontend migration** — move off the untested `apps/` webapps toward a
gpui desktop app so the UI lives in the same tested Rust tree as the engine
(see `roadmap/blocked.md`). This is the structural fix for the neglected-UX
problem: the UI can no longer hide in an untested folder.
2. **Surface each control flow** in `roadmap/ux.md` — pace modes, accept /
   skip / stop, outline editor, chapter steering, commentary, story
   direction, revision-with-diff, persistence — each with a real UI and a
   test proving a human can drive it.
3. **Per-feature tests.** No surface ships without a test. This is the
   Definition of Done (see `roadmap/README.md`); it is non-negotiable.
4. **Keep the engine green.** `cargo test --workspace` and `cargo clippy
   --workspace --all-targets -- --deny warnings` must stay green. Run them
   before reporting done.

Do **not** resume the old engine todos (per-handler grammar routing,
grammar-coverage audit, new example binaries) unless a human-facing feature
requires it. Those were the ✅-checklist drift we removed.

## Token 0 (EOS) in state-tuning

RWKV training uses token **0 (EOS/end-of-document) as document separator**
(see [RWKV-v5 make_data.py](https://github.com/BlinkDL/RWKV-LM/blob/main/RWKV-v5/make_data.py):
'Here "/" means end_of_doc, which is actually token [0]').

State-tuning functions (`bake_into_session`, `bake_no_think_session`,
`bake_persona`) now feed token 0 between consecutive examples to match
this training distribution. Before this fix, examples were replayed
sequentially without EOS padding, leaving the recurrent state in a
distribution the model was never trained on.

**Mechanism:** `ModelBackend::feed_eos()` sends `ActorMessage::FeedEos`
to the RWKV actor thread, which feeds raw token 0 via
`RnnInputBatch(vec![0u32])` and saves the updated state back to the pool.
Default impl is a no-op (MockBackend).

**Impact:** EOS-padded state-tuning may be sufficient to suppress
`<think>` without generation-time `NO_THINK_PREFILL`. Phase B of
`token0_probe.rs` (`crates/cli/examples/`) tests this hypothesis against
the OLD approach (NO_THINK_PREFILL at generation time). Run:
```bash
RWKV_MODEL=... cargo run --release --example token0_probe -p roco-cli
```
