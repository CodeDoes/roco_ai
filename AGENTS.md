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
│   ├── agent/              # roco_agent — Agent ReAct loop, AgentConfig, AgentTrace
│   ├── chat-common/        # roco_chat_common — Conversation, DisplaySettings
│   ├── cli/                # roco_cli — `roco` bin + examples (chat, eval, agent)
│   ├── tui/                # roco_tui — terminal UI (stub)
│   ├── server/             # roco_server — HTTP server (stub)
│   └── gateway/            # roco_gateway — API gateway (stub)
├── vendor/web-rwkv/        # patched web-rwkv dependency ([patch.crates-io] in Cargo.toml)
├── models/                 # RWKV .st files; on-disk truth for model resolution (gitignored)
├── assets/vocab/           # rwkv_vocab_v20230424.json (the tokenizer)
├── scripts/                # pth_to_st/ and gguf_to_st/ model converters
├── goals/                  # product roadmap (see goals/index.md)
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
| `session` | `pool.rs` | `LruSessionPool` for state save/restore |
| `tools` | `tool.rs`, `registry.rs`, `builtins.rs`, `parse.rs` | `Tool` trait, `ToolRegistry`, 6 built-ins, tool-call parsing |
| `workspace` | `workspace.rs` | `Workspace` sandbox boundary |
| `agent` | `agent.rs`, `subtask.rs`, `error.rs` | `Agent` ReAct loop, `AgentConfig`, `AgentTrace` |
| `chat-common` | `conversation.rs`, `display.rs` | `Conversation`, `DisplaySettings` (shared across frontends) |
| `cli` | `bin/roco.rs` + `examples/` | `roco` binary, `chat`/`eval_suite`/`grammar_smoke`/`agent` examples |
| `tui` | `app.rs`, `widgets/` | Terminal UI (stub) |
| `server` | `server.rs`, `routes.rs` | HTTP server (stub) |
| `gateway` | `gateway.rs`, `router.rs` | API gateway (stub) |

Examples live in each crate's `examples/` dir (`inference`: `rwkv_test`,
`gpu_check`, `quant_analyze`, `style_stress`; `cli`: `chat`, `eval_suite`,
`grammar_smoke`, `agent`; `server`: `daemon`).

## Goals

`goals/` is the product roadmap, organized as prerequisite-ordered layers
from the local RWKV-7 engine up to a full agent:

| Layer | What it covers | State |
|---|---|---|
| `infer/` | inference engine (model, quant, state, decoding, structured output) | ✅ complete (needs GGUF→ST fix for 0.1B/1.5B) |
| `message/` | chat protocol (instructions, formatting, tool calls, chat CLI) | ✅ core (chat_cli + a few items remain) |
| `workspace/` | the environment the agent acts in | ⬜ not started |
| `agent/` | the autonomous agent loop and its capabilities | 🟡 core loop done (planning/memory/etc. remain) |
| `agent_chat/` | persistent workspace or folder-bound agent sessions | ⬜ not started |
| `browser_use/` | driving a real browser | ⬜ not started |
| `testing/` | eval harness, oracles, regression gates | ✅ done |
| `coder/` | **(future)** the agent's own develop/test/lint loop in a controlled sandbox | ⬜ not started |

Each folder contains an `index.md` listing its goals in dependency order. A
goal's prerequisites come before it in that file. Goal files carry only
intent (and optional `User:` notes / reference links); progress lives in
`PROGRESS.md`.

There is also a **`self-directed-goals/`** tree — the agent's own reflection of
this roadmap. It mirrors the layer structure but encodes the agent's
autonomous priorities, sequencing, and engineering commitments (see
`self-directed-goals/index.md`). Product intent stays in `goals/`; the
agent's working plan lives in `self-directed-goals/`.

## Quickstart

```bash
cargo run --bin roco -- eval              # run evals, snapshot saved
cargo run --bin roco -- bless             # bless current snapshot as new oracle
cargo run --bin roco -- rwkv              # smoke-test the RWKV backend
cargo run --bin roco -- grammar           # grammar-constrained decode smoke test
cargo run --bin roco -- gpu-check         # show Vulkan device + model status
cargo test --workspace                    # full test suite (61 passing, 0 warnings)
cargo build --release                     # all crates (release for GPU work)
```

> **The execution environment is always inside `devenv shell`.** The `roco`
> command is defined as a devenv script in `devenv.nix` (`scripts.*.exec`) and
> maps to the corresponding `cargo run -p … --example …` invocation. It is
> always available — you can also run the binary directly via
> `cargo run --bin roco -- <subcommand>`. The model is auto-detected from
> `models/*.st` (symlinked).
>
> **Features are enabled by default.** The `grammar` feature (on `engine` /
> `inference` / `message`) enables the grammar path. All functionality is
> available without `--features`.
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

## Next things

1. ~~JSON-Schema → GBNF converter~~ **Done.** Primitives + enums + objects +
   arrays. (`crates/grammar/src/json_schema.rs`)
2. The 0.1B / 1.5B GGUF→ST shape mismatch in `scripts/gguf_to_st_converter/`
   (`a0/k_a/k_k/v0/w0/x_*` need `[1,1,emb]`, `r_k` needs `(clock_count,head_dim)`).
   Upstream patch needed; without it only the 2.9B works. Tracked as
   `goals/infer/gguf_st_converter`.
3. ~~Dead module cleanup~~ **Done.** Removed `audio.rs`, the `inference/`
   directory, and `capacity.rs`. All tests pass.
4. ~~Cleanup segfault~~ **Fixed.** Actor thread now joins in `Drop`.
5. ~~`bnf_sampler` integration~~ **Done.** `BnfConstraint` is the primary
   grammar engine with schoolmarm fallback. 61 tests pass, 0 warnings.
6. ~~State pool Phase 1~~ **Done.** Session-based save/restore wired
   through the pipeline with LRU eviction. Phase 2 (N-slot GPU pool)
   and Phase 3 (tensor blending) are forward work.
7. ~~Monorepo restructuring~~ **Done.** Split into 13 crates; message layer
   (GBNF, tools, tool-calling, result handling, error recovery) and the
   agent ReAct loop implemented.
