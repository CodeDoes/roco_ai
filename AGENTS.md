# AGENTS.md — RoCo AI

> What you need to know to work in this repo. Present state + future direction.

## Status (2026-07-12)

### ✅ Done
- **Monorepo conversion**: 6 crates (`core/cli/session/workspace/napi/gateway`) in Cargo workspace, `apps/web/` (Next.js 15), `apps/visualizer/` (Vite+React)
- **`build_v7()` root cause identified**: Debug-mode wgpu validation layers cause GPU driver TDR — confirmed on AMD RADV RENOIR and NVIDIA RTX 2050
- **`build_v7()` works in `--release`**: Model loads in ~18s on NVIDIA RTX 2050, ~20s on AMD (not yet tested)
- **End-to-end inference confirmed**: `rwkv_test --release` generates coherent text. 12 prompt tokens → 32 completion tokens in 1.96s (≈16 tok/s on NVIDIA RTX 2050)
- **Model loading path fixed**: `std::fs::read` instead of `Mmap`, Int8 quant default, debug warning added
- **`cargo test --workspace`**: All tests pass
- **`devenv.nix`**: Monorepo-aware with Node 22, corepack, processes for web/gateway/viz

### 🚧 In Progress
- **Cleanup segfault at exit**: Inference works but segfaults on shutdown — wgpu resources dropped in wrong order between actor thread and main thread. Minor: doesn't affect inference quality.
- **AMD iGPU path**: Need to test `build_v7` on AMD RADV RENOIR in release mode (should work)
- **`roco chat` binary**: Need to verify the CLI entry point runs end-to-end (pipes through agent loop)

### ❌ Blocked
- *(nothing currently blocked — inference works)*

## What This Is

RoCo AI is a Rust agent framework using **RNN/RWKV/SSM** architectures. The core idea: a **structured, replayable execution trace** is the primary artifact of every run — replacing "screenshot + pass/fail" with diffable execution history.

## Workspace Layout

```
roco_ai/
├── Cargo.toml              # workspace: core/cli/session/workspace/napi/gateway
├── Cargo.lock
├── crates/
│   ├── core/               # roco_core — engine, agent, memory, tools, vector, trace, rwkv_backend
│   ├── cli/                # roco binary: demos A–F, chat, viz, eval, session, trace
│   ├── session/            # roco_session — Engine (message queue + poll loop)
│   ├── workspace/          # roco_workspace — managed filesystem for agent sessions
│   ├── napi/               # roco_napi — napi-rs .node addon (cdylib, +package.json)
│   └── gateway/            # roco-gateway — axum HTTP server (POST /rpc, SSE)
├── apps/
│   ├── web/                # @roco/web — Next.js 15 frontend (chat + traces + oRPC)
│   └── visualizer/         # @roco/visualizer — React+Vite standalone visualizer (4 scenes)
├── model/                  # backend configs (orchestrator/worker/verifier_config)
├── provider/               # adapter descriptors (nvidia/kilo/local_rwkv/mock)
├── agents/                 # agent role scaffolding (orchestrator/worker/verifier)
│   └── README.md           # describes agent directory structure
├── evals/                  # eval suite definitions
├── scripts/                # pth_to_st_converter/ — PTH ↔ SafeTensors conversion
├── models/                 # RWKV model files (.st SafeTensors, 5.5GB)
├── assets/vocab/           # tokenizer vocabulary (rwkv_vocab_v20230424.json)
├── docker/                 # Dockerfile.gateway + Dockerfile.web
├── devenv.{yaml,nix}       # Nix dev environment (Rust + Node + Vulkan)
├── .envrc                  # direnv → devenv auto-load
├── .env                    # API keys (KILO_API_KEY, NVIDIA_API_KEY)
├── Makefile                # build/test/run targets
├── pnpm-workspace.yaml     # JS workspace: apps/*
├── package.json            # root JS scripts (dev:next, dev:visualizer, etc.)
└── flake.nix               # Nix flake (legacy, devenv is the primary shell)
```

## Quickstart

```bash
# Enter dev shell (direnv auto-loads if installed, otherwise):
devenv shell

# Build all Rust:
cargo build --workspace

# Run all Rust tests:
cargo test --workspace

# Run the CLI:
cargo run --bin roco -- chat

# Start the web app (separate terminal):
cd apps/web && pnpm dev

# Start the visualizer (separate terminal):
pnpm dev:visualizer

# Start the gateway (separate terminal):
cargo run -p roco-gateway
```

## RWKV GPU Commands

Model is 5.5 GB SafeTensors at `models/rwkv7-g1g-2.9b-...-converted.st`.
Vocab at `assets/vocab/rwkv_vocab_v20230424.json`.

### GPU capability check (quick, no model load)

```bash
cargo run -p roco-core --features local-rwkv --example gpu_check
```

Scans Vulkan adapters, checks cooperative matrix support, recommends `RWKV_QUANT`.

### RWKV load test (stage-by-stage with 30s timeout per stage)

```bash
# Release build (required! debug builds hang on many GPUs)
RWKV_QUANT=32 \
cargo run -p roco-core --features local-rwkv --example rwkv_load_test --release
```

Stages: file read (5.5 GB → RAM) → SafeTensors deserialize → Loader::info →
adapter enumeration → context creation → quant setup → build_v7
(model weights → VRAM). Each stage has a 30s timeout.

### Run the full CLI with GPU backend

```bash
cargo run --bin roco -- chat --release
```

### Quantization notes

- **Default: Int8 for all 32 layers**. The 2.9B model (5.5 GB FP16) doesn't fit
  in 4 GB VRAM unquantized. Int8 halves to ~2.75 GB.
- **NF4** requires cooperative matrix support (NVIDIA GPUs with tensor cores
  or llvmpipe CPU fallback). Set `RWKV_QUANT=nf4=32`.
- **No quantization**: `RWKV_QUANT=none` (only works if model fits in VRAM,
  or weights stream through VRAM at ~700 MB resident).
- Override GPU selection: `RWKV_ADAPTER=NVIDIA roco chat`
  (substring match against adapter name).

### ⚠ Debug-mode GPU hang

`build_v7()` hangs indefinitely in **debug** builds on many GPU/driver
combinations (AMD RADV RENOIR iGPU, NVIDIA RTX 2050 discrete GPU have both
been confirmed). This is NOT a bug in our code — it's caused by:

1. **wgpu validation layers** enabled in debug builds, which add overhead
2. **Unoptimized CPU code** — tensor processing is 10-100× slower in debug
3. **GPU driver TDR** (Timeout Detection & Recovery) — the driver kills the
   GPU context when submissions are too far apart, and `device.poll()` with
   `timeout: None` never returns on a lost context

**Always use `--release` for GPU inference.** Release builds complete `build_v7`
in ~18 seconds (NVIDIA RTX 2050, Int8 quant).

For reference: the rwkv-harness project uses web-rwkv v0.10.20 through napi-rs
(Node addon), which always compiles in release mode, so it never encounters
this hang.

### Debug build workaround

The code prints a warning in debug builds:
```
WARN  rwkv_backend] Debug build detected! build_v7() may hang on some GPUs.
                    If this hangs, rebuild with `--release`.
```

If you must use a debug build, try:
- Force CPU adapter: `RWKV_ADAPTER=llvmpipe` (very slow, but won't hang)
- Use a smaller model at `RWKV_MODEL=...` (not yet available)
- Set `RWKV_QUANT=none` and hope the model streams through VRAM faster than TDR

## Next Steps

1. **Fix cleanup segfault** — ensure wgpu resources are dropped on the correct thread (actor thread shutdown ordering)
2. **Test `roco chat` binary** — verify the CLI entry point (agent loop) pipes through RwkvBackend correctly
3. **Test on AMD iGPU** — confirm everything works on AMD RADV RENOIR in release mode
4. **Benchmark inference speed** — tokens/s on NVIDIA RTX 2050 vs AMD iGPU vs CPU (llvmpipe)
5. **Test `prepare_ram` → `unbind_gpu` → `bind_gpu` cycle** (VRAM eviction workflow from rwkv-harness)
6. **Fix inference output quality** — current output is repetitive (same sentence multiple times); might need temperature tuning or stop-condition fix

---

## Crate Map — Source Files & What They Do

### `crates/core/src/` — Everything lives here

| File | Purpose | Key Types/Traits |
|---|---|---|
| `engine.rs` | Model inference seam | `ModelBackend`, `CompletionRequest`, `CompletionResponse`, `MockBackend`, `TokenCounter` |
| `agent.rs` | Orchestrator-Worker decomposition | `Orchestrator`, `Worker`, `Task`, `Subtask`, `Verifier`, `ChecklistVerifier`, `ContextBudget`, `RetryPolicy`, `EscalationController` |
| `config.rs` | Provider selection from JSON config | `Config`, `Provider` enum (Nvidia/Kilo/Mock/LocalRwkv), `build_backend()` |
| `backends.rs` | `AnyBackend` dispatcher + HTTP backends | `AnyBackend`, `NvidiaBackend`, `KiloBackend`, `LocalRwkvBackend` (placeholder) |
| `rwkv_backend.rs` | Real RWKV inference via web-rwkv/WebGPU | `RwkvBackend`, `RwkvActor` (actor thread pattern). ⚠ Debug-mode hang — see "Debug-mode GPU hang" above |
| `rwkv.rs` | RWKV-specific helpers | Tokenizer wrappers, sampling helpers |
| `trace.rs` | Execution trace recording + persistence | `TraceEvent`, `Tracer`, `CollectingTracer`, `Trace`, `TraceStore`, `TraceDiff` |
| `tools.rs` | Tool registry & built-in tools | `ToolRegistry`, `Tool`, `AddTool`, `EchoTool` |
| `builtins.rs` | File I/O, bash, vector store, STT/TTS tools | `default_agent_toolkit()`, `BashTool`, `FileReadTool`, `FileWriteTool`, `VectorSearchTool`, `VectorUpsertTool` |
| `toolcall.rs` | Parse/execute ` | `ToolCallParser`, `ToolCallExecutor` |
| `memory.rs` | Context window management | `ContextBudget`, `WindowSlice` |
| `capacity.rs` | Backend capacity routing | `Capacity`, `Pool`, `select()` |
| `infer.rs` | Inference orchestration | `Inferer`, `InferenceStrategy` |
| `audio.rs` | STT/TTS utilities | `SttEngine`, `TtsEngine` |
| `eval.rs` | Eval harness | `EvalSuite`, `EvalCase`, `EvalRunner` |
| `grammar.rs` | Structured output grammar | `Grammar`, `JsonSchema` |
| `policy.rs` | Agent policy enforcement | `Policy`, `SandboxGuard` |
| `sandbox.rs` | Sandboxed execution | `Sandbox`, `SandboxConfig` |
| `train.rs` | Online learning / fine-tuning | `Trainer`, `TrainExample` |
| `vector.rs` | Vector store (pgvector-like) | `VectorStore`, `Embedding` |
| `visualizer.rs` | HTML trace visualization | `render_trace()`, `render_diff()` |
| `logger.rs` | Structured logging | `LogEvent`, `LogCollector` |
| `test.rs` | Test utilities | `test_backend()`, `fake_trace()` |

### Examples (`crates/core/examples/`)

| Example | Purpose |
|---|---|
| `gpu_check.rs` | Scan Vulkan adapters, check cooperative matrix, recommend quant |
| `rwkv_load_test.rs` | Stage-by-stage RWKV model load validation (30s timeouts) |
| `rwkv_test.rs` | Quick smoke test: load model, generate a few tokens | ✅ Works in `--release` (16 tok/s on RTX 2050) |
