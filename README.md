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

### Implemented foundation (compiles & tested without a model)

The orchestration layer is built and exercisable via a `MockBackend`; swap in a
real 3B `ModelBackend` once the model is downloaded.

| Module | Purpose | Status |
|--------|---------|--------|
| `engine.rs` | `ModelBackend` trait (the model seam) + `MockBackend`, token budget/counting | Done |
| `agent.rs` | Orchestrator-Worker: 4K `ContextBudget`, schema-first `Worker`, verification gates (`ChecklistVerifier`, `JudgeVerifier`), escalation cascade, retry circuit breakers, fan-out + aggregation | Done |
| `capacity.rs` | Capacity model + `CapacityPool` + backend routing (`BackendKind`, `select`) — decision policy for where subtasks run | Done |
| `config.rs` | `Config` (provider selection, capacity, retry, context) loaded from `model/default_config`; **default provider: NVIDIA** | Done |
| `backends.rs` | HTTP model backends (OpenAI-compatible): `NvidiaBackend` (free NVIDIA API), `KiloBackend`, `AnyBackend` — gated behind `http-backends` | Done (feature-gated) |
| `main.rs` | Smoke test: decomposition, passing gate, escalation-to-human, and optional live backend demos | Done |
| `tools.rs` | `Tool` trait + `ToolRegistry` (register/lookup/schemas/validate/dispatch) | Done |
| `grammar.rs` | GBNF generation for tool calls (`tools_to_gbnf`/`_with_think`/`_response`), `tools_to_xml`, `validate_grammar` | Done |
| `sandbox.rs` | Timeout-bounded command runner + `GuardPolicy` (deny/allowlist) gate | Done |
| `policy.rs` | Composable `Policy` gate over actions (sandbox guard, tool allowlist, human-in-loop) | Done |
| `toolcall.rs` | Parse `<tool_call>` from model output → vet via policy → dispatch via registry/sandbox | Done |
| `builtins.rs` | Concrete agent tools: `read`/`write`/`list` (workspace-rooted) + `bash` (via sandbox) | Done |

Design patterns follow `models/small_model_agent_patterns.md`. Run the smoke test with:

```bash
cargo run --bin roco
cargo test
```

### Real model backends (feature-gated)

The `http-backends` cargo feature adds OpenAI-compatible HTTP backends that
implement `ModelBackend`, so they drop straight into the `Orchestrator`:

| Backend | Env vars | Default endpoint |
|---------|----------|-----------------|
| `NvidiaBackend` | `NVIDIA_API_KEY` (or `NVAPI_KEY`); opt. `NV_MODEL` | `https://integrate.api.nvidia.com/v1` (free NVIDIA API) |
| `KiloBackend` | `KILO_API_KEY` (opt. `KILO_BASE_URL`, `KILO_MODEL`) | `https://api.kilo.ai/api/gateway` (OpenAI-compatible) |
| | default model `tencent/hy3:free`, `medium` reasoning effort | |

```bash
# build / test with the backends compiled in
cargo build --features http-backends
cargo test  --features http-backends

# run the live demos (keys are read from env or a local .env file)
cargo run --features http-backends
# or inline:
NVIDIA_API_KEY=... KILO_API_KEY=... cargo run --features http-backends
```

Keys are loaded from the environment; a local `.env` file (e.g.
`KILO_API_KEY=...`, `NVIDIA_API_KEY=...`) is also picked up automatically via
`dotenvy` when the `http-backends` feature is enabled.

`NvidiaBackend` curated models (`NvidiaBackend::MODELS`): `qwen/qwen3-next-80b-a3b-instruct`
(default), `nvidia/nemotron-3-super-120b-a12b`, `z-ai/glm-5.2`, `minimaxai/minimax-m3` —
select any via `NV_MODEL`. It requests JSON mode (`response_format`) since Nemotron
supports it; if a provider rejects that field, construct with `.with_json_mode(false)`.
`KiloBackend` targets the confirmed Kilo AI Gateway at `https://api.kilo.ai/api/gateway`
(OpenAI-compatible, per `kilo.ai/docs/gateway`). Model names are provider-prefixed
slugs; the default is `tencent/hy3:free` with `medium` reasoning effort (the
`reasoning_effort` field is forwarded to the underlying reasoning model). Override
with `KILO_MODEL`.

### Local RWKV backend (future)

Local models (rwkv7_g1g 1.5B–13B) will run via a local backend with three
execution modes — `gpu_direct_quantized` (loaded onto GPU from disk),
`gpu_partial_offload` (CPU↔GPU hybrid), and `cpu_only` — keeping weights in
cache for reuse. Design captured in `provider/local_rwkv_adapter`; reference
implementation: <https://github.com/cryscan/web-rwkv>.

### Configuration

Provider, capacity, retry, and context settings are driven by `src/config.rs`,
loaded from `model/default_config` (JSON). The default provider is **NVIDIA**;
set `"provider": "kilo"` (or `"mock"`) to switch. Provider descriptors live in
`provider/*_adapter` and per-role settings in `model/{worker,orchestrator,verifier}_config`.
