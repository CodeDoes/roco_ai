# AGENTS.md — RoCo AI

> Operational manual for working in this repo.

## What this is

A Rust workspace where the only currently-active inference path is
`crates/core/src/rwkv_backend.rs` (RWKV-7 via `web-rwkv` + WGPU).
Everything else (orchestrator, gateway, web frontend, etc.) is
compiled but not the focus right now — we are pushing the small
local RWKV model as far as we can.

## Status

- **Inference**: works end-to-end on `RWKV-7 g1g 2.9B` (FP16 SafeTensors
  → quantized to NF4 at runtime on RTX 2050 / AMD iGPU).
- **Grammar-constrained decoding**: plumbing in place
  (`grammar-rwkv` feature on `roco-core` → schoolmarm GBNF walker
  restricts logits at every sample step). Three hand-written GBNF
  eval cases (`eval_suite::grammar_eval_cases()`) plus a
  `grammar_smoke` example binary. JSON-Schema -> GBNF converter
  is the remaining piece (see Next things).
- **Model loading**: `crates/core/src/rwkv_backend.rs` auto-detects
  model shape from `Loader::info`, picks a quantization plan from
  on-disk file size, and resolves model paths from
  `$RWKV_MODEL` / `models/*.st`.
- **Cleanup segfault**: `free(): invalid size` at process exit
  (wgpu drop-order across threads). Non-fatal for inference.

## Layout

```
roco_ai/
├── Cargo.toml              # workspace: core/cli/session/workspace/napi/gateway/infer
├── crates/
│   ├── core/               # roco_core — engine, agent, eval_suite, grammar, rwkv_backend
│   ├── cli/                # roco — demos, chat, eval, session, trace
│   ├── session/            # roco_session — message queue + poll loop
│   ├── workspace/          # roco_workspace — sandboxed FS for sessions
│   ├── napi/               # roco_napi — .node addon
│   ├── gateway/            # roco-gateway — axum HTTP (POST /rpc, SSE)
│   └── infer/              # roco-infer — OpenAI-shaped HTTP front-end
├── apps/{web,visualizer}   # (kept, but not the focus)
├── models/                 # RWKV .st files; on-disk truth for model resolution
├── assets/vocab/           # rwkv_vocab_v20230424.json (the tokenizer)
├── scripts/                # pth_to_st/ and gguf_to_st/ converters
├── devenv.{yaml,nix}       # Nix dev shell (rust + node + Vulkan)
└── .env                    # local API keys (gitignored in practice)
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

## Quickstart

```bash
devenv shell                            # or `nix develop` if no direnv

cargo build --workspace                 # all crates (release for GPU work)
cargo test --workspace                  # 98 tests, all passing as of last commit
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
3. Clean up the dead modules in `crates/core/src/` (`audio`, `infer`,
   `capacity`, `resource`) — they compile, have no external consumers,
   and currently inflate the test graph.
4. Investigate the cleanup segfault; not blocking inference, but ugly.
</content>
