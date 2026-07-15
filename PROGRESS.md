# PROGRESS.md — RoCo AI

> Strategy / context / "what we wanted to do but didn't yet".
> Living document; this version reflects the rwkv7-first scope as of 2026-07-14.

## Current scope

The active focus is **rwkv7** — push the local RWKV-7 g1h family as the
single backbone model for everything we can. The only working inference
path today is `crates/inference/src/backend.rs` (web-rwkv + WGPU +
SafeTensors). Other backends (mock / HTTP) exist for tests but are not
the product.

When the local model isn't good enough, the escape hatch is the Story
Agent harness from `~/Documents/dev/ksr/` — its `infer/` stack inherits
`rwkv_backend.rs` unchanged and adds grammar-constrained generation on
top. We don't pursue non-rwkv7 engines (candle, llama.cpp, mistral.rs,
LiteRT).

The product roadmap lives in `goals/` — indexed by `goals/index.md` and
AGENTS.md — as prerequisite-ordered layers: `infer`, `message`,
`workspace`, `agent`, `agent_chat`, `browser_use`, `testing`, plus
future `coder`. This file is the strategy/context layer (the "why",
dead-ends, run book); the actionable roadmap is `goals/`.

The agent's **own** working plan is `self-directed-goals/` — a reflection of
`goals/` that encodes the agent's autonomous priorities, sequencing, and
engineering commitments (keep the build green, wire features end-to-end, grow
eval coverage, and ultimately let the agent steer itself).

### Completed priorities

**BNF / Grammar-constrained decoding — ✅ DONE.** The `BnfConstraint`
module (`crates/grammar/src/bnf.rs`) wraps `bnf_sampler` (v0.3.8)
with a `qp-trie` vocabulary built from the model's tokenizer. It is the
primary grammar engine in `rwkv_backend.rs`, with schoolmarm as a
transparent fallback for GBNF grammars that use features `bnf_sampler`
can't parse (character classes `[...]`, quantifiers `*`). The GBNF→BNF
converter wraps nonterminal names in angle brackets so `bnf_sampler`'s
parser accepts them.

**State-mixing / State pool — ✅ Phase 1 DONE.** Session-based
save/restore is wired through the entire pipeline:
`CompletionRequest::session` → `CompleteReq::session` →
`RwkvActor::handle_complete`. Before generation the actor loads the saved
session state (or blank initial state); after generation it reads the
state back via `AnyState::back()` and stores it in the LRU pool. The pool
evicts least-recently-used sessions when it exceeds `max_sessions`
(default 8). Phase 2 (multi-slot GPU pool with concurrent batching) and
Phase 3 (tensor-level state blending) are forward work.

**Chat CLI — ✅ DONE.** `crates/cli/examples/chat.rs` provides a terminal
REPL with streaming output, session persistence (`session: "chat"`),
grammar constraints (`/grammar <file>`), temperature control, and Ctrl+C
interrupt. Invoked via `cargo run -p roco-cli --example chat --release`.
There is also a `roco` binary (`crates/cli/src/bin/roco.rs`).

**Monorepo restructuring — ✅ DONE.** The monolithic `crates/core` was
split into 13 focused crates: `engine`, `grammar`, `inference`, `message`,
`session`, `tools`, `workspace`, `agent`, `chat-common`, `cli`, `tui`,
`server`, `gateway`. `infer` layer is complete (raw model, tokenization,
quantize, inference, streaming, GBNF, structured output + objects, thinking,
state save/load/mix, interrupt, continue). `testing/eval_harness` is done.

**Message layer — ✅ DONE (core).** `crates/message/src/gbnf.rs` generates
the structured chat GBNF (`message_format_gbnf` + `assistant_response_gbnf`,
schoolmarm-compatible, with think / tool_tag variants). `crates/tools` has 6
built-in tools (read/write/search/list/bash/now) with JSON schemas, a
`ToolRegistry`, and `parse` helpers that extract `<tool_call>` blocks and
segment assistant output. `crates/message/src/error.rs` provides
`complete_with_retry` (grammar fallback, truncation handling, backoff).

**Agent loop — ✅ DONE (core ReAct).** `crates/agent/src/agent.rs` runs the
observe→think→act loop: render prompt → constrained generate → parse
segments → execute tools via `ToolRegistry` → feed `<tool_result>` back →
repeat until final answer or step/budget limit. `AgentConfig` /
`AgentStep` / `AgentTrace` record the run. Runnable via
`cargo run -p roco-cli --example agent --release`.

## Model loading strategy

```
hardware scan → resolve model path → quantize for VRAM → build context → generate
```

- **Auto-resolution**: `$RWKV_MODEL` env var → first `rwkv7-*.st` under
  `models/` or `../models/` → error listing what was on disk.
- **Auto-quantization**: reads `Loader::info` for layer count + embedding,
  reads the on-disk FP16 file size as ground truth (wgpu's
  `max_buffer_size` over-reports on NVIDIA RTX 2050 by 200×).
  Policy: `< 1.5 GB` → no quant; `≥ 1.5 GB` (and `gpu_coop`) → NF4;
  `≥ 1.5 GB` (no `gpu_coop`) → Int8; otherwise no-quant.
- **Pipeline caches** under `/tmp/roco-pipeline-cache/` keyed by model
  hash speed up subsequent loads.

## Architecture map (the rwkv critical path)

Concrete request flow on the current code, end-to-end.

```
clap / napi / axum (entries)
        |
        v
crates/engine/src/eval::run_suite              <- 10 default cases live here
crates/engine/src/backend::ModelBackend::complete  <- trait, code-path-agnostic
        |
        v
crates/inference/src/backend::RwkvBackend::complete
   sends CompleteReq over mpsc::Sender
        |
        v
RwkvActor thread (crates/inference/src/actor.rs; LocalSet + current-thread tokio)
   owns Context, TokioRuntime<Rnn>, AnyState, token_strings
   * tries BnfConstraint (crates/grammar/src/bnf.rs) for grammar; falls back to schoolmarm
   * loads session state or blank initial state
   * prompt tokens -> softmax_one -> sample_token
        + grammar constraint masks disallowed indices to -inf
        + accept_token advances state after each sample
   * saves session state back after generation
   * decodes via web_rwkv::Tokenizer
        |
        v
CompletionResponse.text -> caller
```

The actor-thread split exists because `web-rwkv`'s async methods
produce non-`Send` futures (they embed wgpu resources). The
`build_v7` weight upload happens once at thread spawn; afterwards
the actor is a request server. `mpsc::Sender::send` from the calling
thread is `Send`, so callers can be anywhere.

### Decisions baked into the rwkv critical path

| Decision | Why |
|---|---|
| `std::fs::read`, not `memmap2` | Mmap crashed producer on the AMD iGPU. The 5.5 GB FP16 read is cached by the kernel page cache. |
| `LocalSet` + current-thread tokio on a dedicated OS thread | web-rwkv's async methods produce non-Send futures (they embed `wgpu::Device`). `mpsc::Sender` is Send across threads. |
| Session state pool with LRU | Independent evaluations need clean state; sessions need persistent state. The pool handles both: `session=None` → blank state, `session=Some(id)` → load/save. |
| Grammar state is fresh per `complete()` | `schoolmarm::GrammarState` isn't `Sync` and only meaningful for one turn. |
| Bytes → UTF-8 PUA mapping (`U+E000..U+E07F`) | Schoolmarm expects UTF-8 strings; non-ASCII BPE bytes round-trip through that PUA range. |
| NF4 / Int8 / no-quant from on-disk file size | wgpu's `max_buffer_size` over-reports (200x on RTX 2050); on-disk size is ground truth. |
| Filter tokens with `NEG_INFINITY` | Avoids a second sample implementation; `-inf` rank-orders below any real probability. |
| Allow passthrough on grammar accept failure | BPE chunkings straddle literal boundaries; "didn't advance" is the right semantics. |
| BnfConstraint first, schoolmarm fallback | bnf_sampler handles most grammars cleanly; schoolmarm catches edge cases (character classes, quantifiers). |

## What fits on this hardware

Real-world measurements on the dev-kit (NVIDIA RTX 2050, 4 GB VRAM):
the 2.9 B model (~5.5 GB FP16) loads with NF4 quantization, lands
in roughly 1.4 GB VRAM resident, and generates at ~16–20 tok/s.
Smaller rwkv7 checkpoints (0.1 B / 1.5 B) are still GGUF;
converting them to ST hits a known tensor-layout bug in
`scripts/gguf_to_st_converter/` (tracked as `goals/infer/gguf_st_converter`).

The actual VRAM ceiling for unquantized FP16 is `4 GB on RTX 2050`,
the actual reported cap from wgpu's `max_buffer_size` is `1 TB`.
We pick quantization from on-disk file size alone, not the adapter's
self-report.

## Status verification (what we've actually run recently)

- **End-to-end inference**: `cargo run -p roco-inference --example
  rwkv_test --release` → model loads, answers in 0.6–1.5 s at ~16–20 tok/s.
- **Chat CLI**: `cargo run -p roco-cli --example chat --release` →
  streaming REPL with session persistence.
- **Agent loop**: `cargo run -p roco-cli --example agent --release -- "<task>"`
  → ReAct loop with tool dispatch.
- **End-to-end eval**: `cargo run -p roco-cli --example eval_suite
  --release -- --backend rwkv` → runs eval cases, writes JSON report to
  `evals/results/latest.json`.
- **Tests**: `cargo test --workspace` → 61 passing, 0 failing.
- **Compiler clean**: `cargo check --workspace --all-targets` — zero warnings.

## Eval framework (`eval_suite.rs`)

`crates/engine/src/eval.rs` + `crates/cli/examples/eval_suite.rs`.
The harness runs `EvalCase` records against any `ModelBackend`
(`mock`, `rwkv`, …) and writes a structured JSON report.

Built-in categories: smoke, instruction following, coherence,
repetition, throughput, format, context. The example binary takes
`--backend` and `--filter` flags.

### Grammar-constrained variant

- `roco_engine::CompletionRequest::grammar: Option<String>`. Any
  backend can carry the field; `RwkvBackend` honors it.
- `RWKV_GRAMMAR` / `RWKV_GRAMMAR_FILE` env vars for scripting.
- The `eval_suite` module exposes `grammar_eval_cases()` (hand-written
  GBNF) and `jsonschema_eval_cases()` (JSON Schema → GBNF chain).

## Next things

The live product roadmap is `goals/` (see `goals/index.md`) —
prerequisite-ordered layers from the inference engine up to a full
agent, plus future `coder`. AGENTS.md is the operator's view (build
flags, env vars, run commands); this file is the strategy context.

### Roadmap alignment (PROGRESS ↔ goals/)

- `infer/*` — **complete** (raw model, tokenization, quantize, inference,
  streaming, GBNF, structured output + objects, thinking, state
  save/load/mix, interrupt, continue). **Blocked:** 0.1B / 1.5B by the
  GGUF→ST shape bug (`goals/infer/gguf_st_converter`).
- `message/*` — **complete (core)**: `message_format_gbnf`, tool catalogue,
  tool calling, tool result handling, error recovery. **chat_cli done**
  (now uses `CompletionRequest::session` for real multi-turn state plus
  `/save` `/load` `/system`). **gradual_tool_disclosure done**
  (`goals/message/gradual_tool_disclosure`): `select_relevant`
  (`crates/agent/src/tool_selector.rs`) discloses only task-relevant tools
  (keyword-overlap score reusing the `memory` ranker), with a safety net
  returning all tools when none score above zero; wired via
  `AgentConfig::gradual_tool_disclosure`. Remaining: `state_tune_examples`,
  `system_instruction_following`, `user_message_response`.
- `agent/*` — **complete**: core loop + tool execution loop done
  (`goals/agent/agent`, `goals/agent/tool_execution_loop`). **Memory done**
  (`goals/agent/memory`), **Planning done** (`goals/agent/planning`), **Orchestrate done**
  (`goals/agent/orchastrate`), **Session search done** (`goals/agent/session_search`),
  **Scheduled tasks done** (`goals/agent/scheduled_tasks`). **Wired end-to-end**:
  the `agent` CLI example now builds a `Workspace`-sandboxed agent with
  `MemoryStore` + `SessionStore` + `Scheduler` combined tools, records its run,
  and runs due tasks; a `MockBackend` integration test asserts the combined
  registry carries every scoped tool and the scheduler integrates with the
  backend. The `agent` layer is now **complete**; remaining integration items
  are in the `workspace`/`message` layers.
- `testing/eval_harness` — **done**.
- `workspace` — **implemented (core)**: `Workspace` sandbox boundary
  (`crates/workspace/src/workspace.rs`) with path-escape protection
  (lexical `..` normalization + canonical-prefix check for existing files),
  `WorkspaceKind` (eval/temp/user/agent/generic), cwd, and `metadata()`.
  Workspace-scoped tools (`Workspace::scoped_tools`) cover file read/write/
  edit/search/list + a cwd-scoped `bash` tool (`crates/workspace/src/tools.rs`),
  all resolving through `Workspace::resolve` so they cannot leave the root.
  The agent drives them via `Agent::with_tools(config, Workspace::scoped_tools(ws))`.
  Caveat: the `bash` tool is cwd-scoped, not a full syscall sandbox.
  **Symlink hardening + sandbox-escape regression guard added**:
  `crates/workspace/src/workspace.rs` now has a dedicated test module that
  plants a secret outside the root and asserts neither lexical `..` traversal
  nor symlink escapes (unix) reach it through `resolve()` or the `read` tool,
  while legitimate in-bounds access still works. **Workspace presets added**:
  `Workspace::preset`/`preset_in` pick conventional roots (`.roco/workspace/agent`
  for `Agent`, base dir for `User`, temp dir for `Eval`/`Temp`/`Generic`).
  **Bash denylist added**: `blocked_command_reason` refuses a conservative set
  of destructive/escape-prone commands; `WorkspaceBashTool` enforces it.
- `agent_chat`, `browser_use`, `coder` — forward-looking; not yet in code.

## Open questions

None outstanding.

## Things we tried that didn't work

### Debug-mode rwkv build hangs

`build_v7()` hangs indefinitely in **debug** on most consumer GPUs
(RTX 2050 / AMD RADV RENOIR confirmed). Cause: wgpu debug-build
validation layers + slow unoptimized shader compilation interacting
with the GPU driver's TDR. Fix: build with `--release`. Fallback:
`RWKV_ADAPTER=llvmpipe` (slow but works).

### 2.9B at FP16 OOMs the RTX 2050

The model's FP16 file is 5.6 GB; the card has 4 GB VRAM. We initially
trusted wgpu's `max_buffer_size` to drive the auto-quant heuristic.
The RTX 2050 reports 1 TB — clearly wrong. We now read **on-disk file
size** as the source of truth: files ≥ 1.5 GB always quantize.

### GGUF → ST converter drops 3-D / matrix shapes

`scripts/gguf_to_st_converter/convert.py` converts 0.1B / 1.5B model
files into SafeTensors, but tensors carry 1-D vectors where web-rwkv
expects 3-D `[1, 1, emb]` matrices for `a0/k_a/k_k/v0/w0/x_*`, and
flat `[emb]` where it expects `(clock_count, head_dim)` for `r_k`.
The 2.9B `.st` on disk is in the right layout — that's why it works.

### mmap-based model load crashed on AMD iGPU

`memmap2` Mmap of the 5 GB model file worked on NVIDIA but hung or
segfaulted on the AMD RADV iGPU. Switched to `std::fs::read`.

## Run book

```bash
# Build everything
cargo build --workspace --release

# Run all tests
cargo test --workspace

# Spot-check GPU adapters
cargo run -p roco-inference --example gpu_check

# Smoke-test: ask the model "capital of France?"
cargo run -p roco-inference --example rwkv_test --release

# Run the eval suite
cargo run -p roco-cli --example eval_suite --release -- --backend rwkv

# Chat REPL
cargo run -p roco-cli --example chat --release

# Agent loop (ReAct with tools)
cargo run -p roco-cli --example agent --release -- "<task>"

# Grammar-constrained smoke
RWKV_GRAMMAR='root ::= "yes" | "no"' \
cargo run -p roco-cli --example grammar_smoke --release
```
