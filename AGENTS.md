# AGENTS.md — RoCo AI

> What you need to know to work in this repo. Present state + future direction.

## What This Is

RoCo AI is a Rust agent framework using **RNN/RWKV/SSM** architectures. The core idea: a **structured, replayable execution trace** is the primary artifact of every run — replacing "screenshot + pass/fail" with diffable execution history.

## Workspace Layout

```
roco_ai/
├── Cargo.toml              # workspace: core/cli/session/workspace/napi/gateway + gui
├── crates/
│   ├── core/               # roco_core — engine, agent, memory, tools, vector, trace, rwkv_backend
│   ├── cli/                # roco binary: demos A–F, chat, viz, eval, session, trace
│   ├── session/            # roco_session — Engine (message queue + poll loop)
│   ├── workspace/          # roco_workspace — managed filesystem for agent sessions
│   ├── napi/               # roco_napi — napi-rs .node addon (cdylib)
│   └── gateway/            # roco-gateway — axum HTTP server (POST /rpc, SSE)
├── gui/                    # Dioxus desktop visualizer (EXCLUDED from workspace)
├── model/                  # backend configs (orchestrator/worker/verifier_config)
├── provider/               # adapter descriptors (nvidia/kilo/local_rwkv/mock)
├── agents/                 # agent role scaffolding (orchestrator/worker/verifier)
├── evals/                  # eval suite definitions
├── scripts/                # pth_to_st_converter/ — PTH ↔ SafeTensors conversion
├── models/                 # RWKV model files (.st SafeTensors, 5.5GB)
└── assets/vocab/           # tokenizer vocabulary (rwkv_vocab_v20230424.json)
```

## Crate Map — Source Files & What They Do

### `crates/core/src/` — Everything lives here

| File | Purpose | Key Types/Traits |
|---|---|---|
| `engine.rs` | Model inference seam | `ModelBackend`, `CompletionRequest`, `CompletionResponse`, `MockBackend`, `TokenCounter` |
| `agent.rs` | Orchestrator-Worker decomposition | `Orchestrator`, `Worker`, `Task`, `Subtask`, `Verifier`, `ChecklistVerifier`, `ContextBudget`, `RetryPolicy`, `EscalationController` |
| `config.rs` | Provider selection from JSON config | `Config`, `Provider` enum (Nvidia/Kilo/Mock/LocalRwkv), `build_backend()` |
| `backends.rs` | `AnyBackend` dispatcher + HTTP backends | `AnyBackend`, `NvidiaBackend`, `KiloBackend`, `LocalRwkvBackend` (placeholder) |
| `rwkv_backend.rs` | **Phase 4: real RWKV inference via web-rwkv/WebGPU** | `RwkvBackend`, `RwkvActor` (actor thread pattern) |
| `trace.rs` | Execution trace recording + persistence | `TraceEvent`, `Tracer`, `CollectingTracer`, `Trace`, `TraceStore`, `TraceDiff` |
| `tools.rs` | Tool registry & built-in tools | `ToolRegistry`, `Tool`, `AddTool`, `EchoTool` |
| `builtins.rs` | File I/O, bash, vector store, STT/TTS tools | `default_agent_toolkit()`, `BashTool`, `FileReadTool`, `FileWriteTool`, `VectorSearchTool`, `VectorUpsertTool` |
| `toolcall.rs` | Parse/execute `