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

- **Inference**: works end-to-end on `RWKV-7 g1g 2.9B` (FP16 SafeTensors
  → quantized to NF4 at runtime on RTX 2050 / AMD iGPU).
- **Grammar-constrained decoding**: plumbing in place
  (`grammar-rwkv` feature on `roco-core` → schoolmarm GBNF walker
  restricts logits at every sample step). Three hand-written GBNF
  eval cases (`eval_suite::grammar_eval_cases()`) plus a
  `grammar_smoke` example binary. JSON-Schema → GBNF converter is
  done (`crates/core/src/jsonschema_to_gbnf.rs`, see Next things #1).
- **Model loading**: `crates/core/src/rwkv_backend.rs` auto-detects
  model shape from `Loader::info`, picks a quantization plan from
  on-disk file size, and resolves model paths from
  `$RWKV_MODEL` / `models/*.st`.
- **Cleanup segfault**: `free(): invalid size` at process exit — **fixed**
  (see Next things #4). wgpu/tokio resources now drop in-order on the
  dedicated actor thread via `RwkvBackend::Drop`.

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
├── goals/                  # product roadmap, numbered by prerequisite (see goals/README.md)
├── evals/results/          # rwkv benchmark JSON outputs
├── devenv.{yaml,nix}       # Nix dev shell (rust + Vulkan)
├── Makefile                # rwkv-focused dev targets
└── .env                    # local API keys (gitignored)
```

The `crates/core/src/` tree holds everything in one flat directory:

| Where it lives | What it does |
|---|---|
| `rwkv_backend.rs` | The only actively-supported inference path. Owns a dedicated actor thread that hosts all non-`Send` WGPU resources. |
| `eval_suite.rs` | Standalone backend eval (smoke, instruction, coherence, format, throughput). The harness the `eval_suite` example binary uses. |
| `engine.rs` | The `ModelBackend` trait + `MockBackend`. Eval runs against any `ModelBackend` impl. |
| `grammar.rs` | GBNF grammar generation from tool schemas (the *receiving* half of grammar-constrained decoding). |
| `agent.rs` / `eval.rs` | Orchestrator pipeline + the wider eval suite (tests via the orchestrator, not just the model). Compiles, runs, but is **not the focus** right now. |
| everything else | Compiled, sometimes exercised by tests. Mostly scaffolding from earlier experiments. Safe to delete on a case-by-case basis — none of it is on the rwkv critical path. |

## Goals

`goals/` is the product roadmap, organized as numbered layers that mirror the
build order from the local RWKV-7 engine up to a full agent:

- `1_infer/` — inference engine (model, quant, state, decoding)
- `2_message/` — chat protocol (instructions, formatting, tool calls)
- `3_workspace/` — the environment the agent acts in
- `4_agent/` — the autonomous agent loop and its capabilities
- `5_browser_use/` — driving a real browser
- `9_coder/` — **(future)** the agent's own develop/test/lint loop in a controlled sandbox

Each file is `NN_name.md`; the numeric prefix is **prerequisite order** — a
file's dependencies come before it (e.g. `tokenization` precedes `inference`;
`tool_catelogue` precedes `tool_calling`; in `9_coder`, `human_approval` is `01`
because the gate must exist before the devloop can run). Files may carry a
`User:` section with notes/constraints added during planning (model variants to
try, tokenizer gotchas, Camoufox for stealth browsing, etc.). `goals/README.md`
is the index. Layers `6`–`8` are intentionally reserved for future categories.

## Quickstart

```bash
devenv shell                            # or `nix develop` if no direnv

cargo build --workspace                 # all crates (release for GPU work)
cargo test --workspace                  # full test suite
cargo run -p roco-core                  # choose a subcommand
cargo run -p roco-core --example eval_suite --release -- --backend rwkv
cargo run -p roco-core --example rwkv_test --release
cargo run -p roco-core --features grammar-rwkv --example grammar_smoke --release
```

## RWKV env vars (read by `rwkv_backend::from_env`)

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

## Next things to consider

1. ~~Add a JSON-Schema → GBNF converter~~
   (`crates/core/src/jsonschema_to_gbnf.rs`). **Done.** Compact
   primitives + enum converter with 5 unit tests, plus a paired
   `eval_suite::jsonschema_eval_cases()` fixture that exercises
   the JSON-Schema → GBNF → schoolmarm chain end-to-end. Object/
   array support is the obvious forward extension but no current
   eval case demands it.
2. The 0.1B / 1.5B GGUF→ST shape mismatch in `scripts/gguf_to_st_converter/`
   (`a0/k_a/k_k/v0/w0/x_*` need `[1,1,emb]`, `r_k` needs `(clock_count,head_dim)`).
   Upstream patch needed; without it only the 2.9B works.
3. ~~Clean up the dead modules in `crates/core/src/` (`audio`, `infer`,
   `capacity`, `resource`)~~ **Done.** Removed `audio.rs`, the `inference/`
   directory (the `infer` stub), and `capacity.rs`, unwiring their consumers in
   `builtins.rs` (`SttTool`/`TtsTool`) and `config.rs` (`CapacityConfig`).
   (`resource` was already gone.) `cargo check --workspace` + the touched unit
   tests pass.
4. ~~Investigate the cleanup segfault~~ **Root-caused + fixed.** Cause:
   the actor thread's `local.block_on` waited on a never-sent oneshot, so the
   thread never exited; its `JoinHandle` was discarded (detached), so at
   process exit the OS killed the thread while it still owned a live tokio
   runtime + wgpu `Context`/`Device`/`Bundle`/`State`. Their allocator state
   was torn down mid-flight, yielding `free(): invalid size`. Fix in
   `rwkv_backend.rs`: await the spawned task's `JoinHandle` (the thread now
   exits once the request channel closes) and join the thread in
   `RwkvBackend::Drop` so wgpu/tokio resources drop in-order on the owning
   thread. Not blocking inference. Not yet runtime-verified on GPU — needs a
   `--release` run to confirm the abort is gone.
</content>
