# AGENTS.md — RoCo AI

> Operational manual for working in this repo.

## What this is

A Rust workspace whose only inference path is `crates/core/src/rwkv_backend.rs`
(RWKV-7 via `web-rwkv` + WGPU). The repo has been pared down to the
local-RWKV critical path — the `crates/core` library plus its `rwkv_*`
examples, the `vendor/web-rwkv` patch, the `scripts/` model converters,
and the `assets/vocab` tokenizer. Everything non-RWKV (orchestrator
crates, gateway/web frontends, Docker, agent/eval scaffolding) has been
removed; git history preserves it.

## Status

- **Inference**: works end-to-end on `RWKV-7 g1h 2.9B` (FP16 PTH → converted
  to SafeTensors → quantized to NF4 at runtime on RTX 2050 / AMD iGPU).
- **Grammar-constrained decoding**: **`BnfConstraint`** (`bnf_sampler`
  v0.3.8 + `qp-trie` vocabulary + GBNF→BNF converter) is the primary
  engine in `rwkv_backend.rs`. Falls back to schoolmarm automatically when
  the GBNF uses features `bnf_sampler` can't parse (character classes `[...]`,
  quantifiers `*`). JSON-Schema → GBNF converter is done
  (`crates/core/src/jsonschema_to_gbnf.rs`).
- **State-mixing / multi-session**: **Phase 1 implemented.**
  `CompletionRequest::session` → session-based state save/restore via
  `AnyState::back()`/`load()`, with an LRU pool (`max_sessions = 8`).
  Enables persistent conversations across calls. Phase 2 (N-slot GPU pool
  with concurrent batching) and Phase 3 (tensor blending) are forward work.
- **Chat CLI**: `roco chat` example provides a terminal REPL with streaming
  output, session persistence, grammar constraints, and Ctrl+C interrupt.
- **Model loading**: `crates/core/src/rwkv_backend.rs` auto-detects
  model shape from `Loader::info`, picks a quantization plan from
  on-disk file size, and resolves model paths from
  `$RWKV_MODEL` / `models/*.st`.
- **Cleanup segfault**: `free(): invalid size` at process exit — **fixed**.
  wgpu/tokio resources now drop in-order on the dedicated actor thread
  via `RwkvBackend::Drop`.

## Layout

```
roco_ai/
├── Cargo.toml              # workspace: crates/core only
├── crates/
│   └── core/               # roco_core — engine, eval_suite, grammar, rwkv_backend (+ examples)
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

The `crates/core/src/` tree holds everything in one flat directory:

| Where it lives | What it does |
|---|---|
| `rwkv_backend.rs` | The only actively-supported inference path. Owns a dedicated actor thread that hosts all non-`Send` WGPU resources. |
| `rwkv_quant_proxy.rs` | RWKVQuant-style proxy analysis: streaming SafeTensors reader, adaptive P_c/P_f per-tensor scoring. |
| `eval_suite.rs` | Standalone backend eval (smoke, instruction, coherence, format, throughput). The harness the `eval_suite` example binary uses. |
| `engine.rs` | The `ModelBackend` trait + `MockBackend`. Eval runs against any `ModelBackend` impl. |
| `bnf_constraint.rs` | Grammar-constrained decoding via `bnf_sampler` with schoolmarm fallback. |
| `jsonschema_to_gbnf.rs` | JSON Schema → GBNF grammar converter. |
| `eval_cases.rs` | Eval case definitions (oracles, categories, prompts). |

Everything outside the `rwkv_backend` critical path has been removed (agent, memory, vector, policy, sandbox, toolcall, tools, handler, config, visualizer, trace, backends, logger, builtins — all dead modules). Git history preserves them.

## Goals

`goals/` is the product roadmap, organized as prerequisite-ordered layers
from the local RWKV-7 engine up to a full agent:

| Layer | What it covers |
|---|---|
| `infer/` | inference engine (model, quant, state, decoding, structured output) |
| `message/` | chat protocol (instructions, formatting, tool calls, chat CLI) |
| `workspace/` | the environment the agent acts in |
| `agent/` | the autonomous agent loop and its capabilities |
| `agent_chat/` | persistent workspace or folder-bound agent sessions |
| `browser_use/` | driving a real browser |
| `testing/` | eval harness, oracles, regression gates |
| `coder/` | **(future)** the agent's own develop/test/lint loop in a controlled sandbox |

Each folder contains an `index.md` listing its goals in dependency order. A
goal's prerequisites come before it in that file. Files may carry a `User:`
section with notes/constraints added during planning (model variants to try,
tokenizer gotchas, Camoufox for stealth browsing, etc.).

## Quickstart

```bash
cargo run --bin roco -- eval              # run evals, snapshot saved
cargo run --bin roco -- bless             # bless current snapshot as new oracle
cargo run --bin roco -- rwkv              # smoke-test the RWKV backend
cargo run --bin roco -- grammar           # grammar-constrained decode smoke test
cargo run --bin roco -- gpu-check         # show Vulkan device + model status
cargo test --workspace                    # full test suite
cargo build --release                     # all crates (release for GPU work)
```

> **The execution environment is always inside `devenv shell`.** The `roco` command
> is defined as a devenv script in `devenv.nix` (`scripts.*.exec`). It is always
> available — do not use `cargo run` directly. Never create a standalone `roco`
> shell script. The model is auto-detected from `models/*.st` (symlinked).
>
> **Features are enabled by default.** The `grammar-rwkv` and `local-rwkv` features
> are in `default = ["grammar-rwkv"]` in `Cargo.toml`. All functionality is
> available without `--features`.
>
> **Snapshot/bless workflow:** Every `roco eval` saves a `.snapshot.json` next to
> the report. When the output is acceptable, run `roco bless` to update the
> source `oracle:` fields, making the current output the new pass/fail reference.

| Variable | Effect | Default |
|---|---|---|
| `RWKV_MODEL` | Absolute path to a `.st` SafeTensors file | First `rwkv7-*.st` in `models/` or `../models/` |
| `RWKV_VOCAB` | Path to vocab JSON | First matching `rwkv_vocab_v20230424.json` next to `RWKV_MODEL` |
| `RWKV_QUANT` | Override auto-quant: `none`, `nf4=N`, or `N` (Int8 N layers) | Auto-picked (NF4 if file ≥ 1.5 GB and GPU has coop matrix; else Int8; else no-quant if file < 1.5 GB) |
| `RWKV_ADAPTER` | Substring match against GPU adapter name | First Vulkan adapter with coop-matrix |
| `RWKV_GRAMMAR` | GBNF grammar to constrain decoding (only if `grammar-rwkv` feature is on) | unset |
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

1. ~~JSON-Schema → GBNF converter~~ **Done.** Compact primitives + enum
   converter. Object/array support is a forward extension
   (`goals/infer/structured_output_objects`).
2. The 0.1B / 1.5B GGUF→ST shape mismatch in `scripts/gguf_to_st_converter/`
   (`a0/k_a/k_k/v0/w0/x_*` need `[1,1,emb]`, `r_k` needs `(clock_count,head_dim)`).
   Upstream patch needed; without it only the 2.9B works. Tracked as
   `goals/infer/gguf_st_converter`.
3. ~~Dead module cleanup~~ **Done.** Removed `audio.rs`, the `inference/`
   directory, and `capacity.rs`. All tests pass.
4. ~~Cleanup segfault~~ **Fixed.** Actor thread now joins in `Drop`.
5. ~~`bnf_sampler` integration~~ **Done.** `BnfConstraint` is the primary
   grammar engine with schoolmarm fallback. 114 tests pass.
6. ~~State pool Phase 1~~ **Done.** Session-based save/restore wired
   through the pipeline with LRU eviction. Phase 2 (N-slot GPU pool)
   and Phase 3 (tensor blending) are forward work.
