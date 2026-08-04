# RoCo AI Codebase Analysis — Crates & Architecture

> **Note**: This document is historical. For current architecture, see [AGENTS.md](AGENTS.md).

## Crates

| Crate | Purpose |
|-------|---------|
| agent | Autonomous agent loop (ReAct + Mechanistic), memory, story pipeline (chapter steering, commentary, outline editing, quality, persistence) |
| app | AppContext, session agent binding, workspace timeline, config |
| cli | CLI commands (eval, story, interact, router, server), thin dispatcher |
| core | Core utilities shared across crates |
| engine | ModelBackend trait, CompletionRequest/Response types, MockBackend, BNF grammar engine, JSON helpers |
| engine-gpu | RWKV-7 Vulkan inference via web-rwkv |
| gateway | HTTP gateway / router |
| harness | Domain harness traits and implementations |
| infer-client | Remote inference client |
| inferd | RWKV inference daemon |
| napi | Node.js native addon bindings |
| protocol | Serialization models and shared data types |
| rwkv7-opencl | OpenCL backend for RWKV-7 |
| rwkv7-triton | Triton backend for RWKV-7 |
| rwkv7-vulkan | Vulkan backend for RWKV-7 |
| server | HTTP server, routes, config |
| session | Persistent session store, sub-sessions, tracing |
| ui | Desktop widgets (chat, markdown editor, pet, session browser) |
| workspace | Sandbox workspace, file access, version control |

## Current Test Coverage

- `crates/app/tests/`
- `crates/cli/tests/` — integration tests (session, story pipeline, WFC, future capabilities)
- `crates/engine/tests/` — grammar tests, eval suite
- `crates/engine/examples/` — full_eval, matrix_eval, root_bake_test
- `crates/engine-gpu/examples/` — full_eval, grammar_smoke, rwkv_test
- `crates/agent/src/evals.rs`
- `evals/results/` — solution_bench.json

All 1005+ tests pass (as of 2026-08-01). See [PROGRESS.md](PROGRESS.md) for latest test counts.

## Historical Gaps (now filled)

The following gaps were identified and addressed:
- Session persistence tests → `crates/cli/tests/session_persistence.rs`
- Story pipeline integration tests → `crates/cli/tests/pipeline_integration.rs`
- WFC tests → `crates/cli/tests/wfc_test.rs`
- Future capabilities tests → `crates/cli/tests/future_capabilities.rs`
- Grammar constraint tests → `crates/engine/tests/grammar_tests.rs`
- Keyword router tests → `crates/cli/src/cmd/router.rs`
