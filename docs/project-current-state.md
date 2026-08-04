# Project Current State: RoCo AI

> **Note**: This document describes the v0.4-v0.5 architecture. For current state, see [AGENTS.md](../AGENTS.md).

This document details the exact architectural, functional, and developer experience (DX) state of RoCo AI as of version 0.4.

## 1. Physical Architecture & Layout

RoCo AI is a unified Rust workspace designed for local AI collaborative writing, worldbuilding, and text editing using **RWKV-7 State Space Models (SSM)**.

### Sub-crates
The codebase was streamlined by consolidating scheduling, orchestration, memory management, and story generation logic into a single central crate: `crates/agent`.

The current crate layout is as follows:
- `crates/agent`: Central agent orchestration, scheduling, memory, validation, and story-generation capabilities.
- `crates/app`: Core logic for managing configurations, environment, and CLI dispatcher body.
- `crates/cli`: Thin dispatcher, REPL command-loop interfaces, terminal formatting.
- `crates/core`: Core utilities shared across crates.
- `crates/engine`: pure-Rust definition of ModelBackend traits, CompletionRequests, and deterministic MockBackend.
- `crates/engine-gpu`: GPU Vulkan inference via `web-rwkv` implementing `ModelBackend`.
- `crates/gateway`: API Gateway reverse-proxy with rate-limiting.
- `crates/harness`: Domain harness traits and implementations.
- `crates/infer-client`: Client for the inference daemon.
- `crates/inferd`: The local model server that loads the model into VRAM and handles inference.
- `crates/napi`: Node.js native addon bindings.
- `crates/protocol`: Serialization models and shared data types.
- `crates/rwkv7-opencl` / `rwkv7-triton` / `rwkv7-vulkan`: Backend-specific RWKV-7 implementations.
- `crates/server`: HTTP API server implementing an OpenAI-compatible `/v1/completions` endpoint.
- `crates/session`: Session management logic.
- `crates/ui` / `crates/workspace`: Desktop-GUI, workspace management, and terminal-UI frontends.

---

## 2. Model Backend & Core Inference

### RWKV-7 State Space Model
- Uses an offline-first **RWKV-7 2.9B parameter Recurrent SSM** with 10k trained context.
- Recurrent execution translates into linear complexity in sequence length and constant VRAM utilization per token, bypassing the quadratic memory scaling of typical Transformer models.
- **Session management as recurrent states**: A session is physically saved/loaded as a recurrent state tensor (vector of floats representing the model's latent state). Saving the state is equivalent to pausing the session, and loading it is instant resumption.

### API Gateway & Server Compatibility
- `roco-server` implements an **OpenAI-compatible `/v1/completions`** API endpoint.
- Direct out-of-the-box integration with standard OpenAI client libraries, Vercel AI SDK, and Assistant UI.
- The Proxy gateway (`crates/gateway`) routes requests and handles basic rate-limiting.

---

## 3. Grammar Engine & Constrained Decoding

To ensure formatting and reliability, RoCo AI enforces grammar-constrained decoding:
- **Production Grammar Engine**: Uses `roco-bnf-engine` wrapping `kbnf` to compile GBNF rules into token masks.
- **Schema Enum Scoping**: Enums are compiled as explicitly named rules (`root_quality_enum ::= ...`) rather than inlined in parent objects. This prevents GBNF `|` low-precedence bugs from bypassing surrounding structure.
- **Actor Generation Loop**: Optimized generation handles grammar masks consistently across both single-token prompt-flush and main-loop phases. Grammar closing tokens are emitted before terminating.

---

## 4. Offline Local Agent Framework & Story Pipeline

The story-writing pipeline consists of 6 distinct sequential phases:
`outline → wiki → chapters → validation → synopsis → publish`

- **Execution Loop**: Implements the `DomainHarness` trait. It includes automatic rollback and retry logic upon validation and generation failures.
- **Strict JSON Parsing**: All pipeline phases require strict JSON outputs under grammar constraints. Parsers fail loudly on format corruption rather than silently falling back to fragile heuristic regex/prose parsers.
- **Sandbox Boundary Safety**: File read and write tools reject relative/absolute path traversal (`..` and `/`) and strictly validate allowed file extensions.

---

## 5. Known Pain Points & Stability Gaps

1. **Model repeating at high temperatures**: The 2.9B model begins repeating significantly at temperatures $\ge 0.7$.
2. **Instruction-following strictness**: Under heavy grammar constraints, string values occasionally degrade or display repetitive patterns, necessitating multiple revision retries.
3. **Flaky mock tests**: Intermittent parallel environment variable leakage causes some port assertion tests to be unstable under aggressive parallelism.
