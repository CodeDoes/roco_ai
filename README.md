# RoCo AI

A Rust-based AI agent framework exploring **RNN**, **RWKV**, and **State Space Models (SSM)** for agentic behavior, context management, and full GPU utilization.

## Vision

Build a self-regulating AI system that can:
- Manage its own context window efficiently using stateful architectures
- Decompose complex tasks into sub-agent operations
- Run inference fully on GPU with minimal CPU-GPU transfer overhead
- Maintain state across long-running sessions without KV-cache bloat

## Architecture

| Module | Purpose |
|--------|---------|
| `agent.rs` | Agent orchestration, task decomposition, sub-agent spawning |
| `engine.rs` | Core inference engine: RWKV / SSM / RNN backends |
| `infer.rs` | Token generation, sampling strategies, batching |
| `train.rs` | Training loop, fine-tuning, RLHF scaffolding |
| `policy.rs` | Self-regulation, safety constraints, decision policies |
| `grammar.rs` | Constrained decoding, structured output generation |
| `sandbox.rs` | Code execution sandbox for tool-use / coding agents |
| `tools.rs` | Tool registry, function calling, external API integration |
| `eval.rs` | Benchmarking, evaluation harnesses |
| `rwkv.rs` | RWKV-specific linear attention implementation |
| `main.rs` | CLI entry point |

## Key Design Decisions

1. **Stateful over KV-cache**: RWKV and SSMs maintain O(1) state per layer, enabling infinite context without memory blowup.
2. **4K instruction context**: Reserve first 4K tokens for system instructions, task specs, and tool schemas; remaining context for generation.
3. **Sub-agent decomposition**: Large tasks split into 4K-chunk sub-tasks for small (3B) models, then aggregated.
4. **Full GPU utilization**: Batch sub-agent requests, use fused kernels, minimize host-device sync.

## Status

Foundation phase. See `REPORT.md` for the full research and design document.
