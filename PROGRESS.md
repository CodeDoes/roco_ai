# PROGRESS.md — RoCo AI

> Ongoing notes, strategy, and wishlist.

---

## Raw Vision

```
hw query → model load strategy → keep in memory via inference-api → lock files (removed on process stop)
```

- **Hardware-first**: query GPU (Vulkan), check VRAM, cooperative matrix → decide what fits.
- **Model zoo**: not just RWKV. `~/Documents/models/` for fast models (<=3B, GPU). Large models on CPU for smarts.
- **Critique loop**: every model's output gets critiqued by another model (or heuristic). Assign each model to what it's strongest at. Fast models propose, smart models critique and direct.
- **FFN engine needed**: RWKV is RNN-only. Need a separate transformer inference engine that loads SafeTensors directly (candle, llama.cpp, mistral.rs, burn).
- **Converter needed**: general `.pth` → SafeTensors conversion script (not just RWKV-specific).
- **Lock file protocol**: `.lock` per model in `/tmp/roco-models/`, removed when process stops (clean or not). Start-up scans for stale locks.

---

## Model Loading Strategy

### Flow

```
hw query → model load strategy → keep in memory via inference-api → lock files (removed on process stop)
```

- **Hardware query**: scan Vulkan adapters, check VRAM, cooperative matrix support → pick optimal quant & device.
- **Model load strategy**: decide which model to load based on task requirements (speed vs. smarts).
- **Keep in memory**: loaded models stay warm behind a lightweight inference API (HTTP or IPC). No per-request load/unload overhead.
- **Lock files**: a `.lock` file per model at e.g. `/tmp/roco-models/<model-hash>.lock`. Removed on clean shutdown. On restart, stale locks are cleaned up. This prevents double-loading and lets other processes know what's resident.

### Local-First Ethos

APIs are a crutch. The backbone is **local inference** — RWKV and other small
models that fit in 4GB VRAM with Int8 quant. API models (NVIDIA, Kilo) are
optional supplements for tasks that exceed local capability, not the default.

### What Fits on Hardware (4GB VRAM NVIDIA RTX 2050 / AMD RADV RENOIR)

| Model | Size | Quant | VRAM | Status |
|---|---|---|---|---|
| RWKV 2.9B | 5.5 GB FP16 | Int8 | ~2.75 GB | ✅ Working (16 tok/s) |
| Qwen2.5-Coder 1.5B | 3 GB FP16 | Int8 | ~1.5 GB | ⏳ Should work |
| TinyLlama 1.1B | 2.2 GB FP16 | Int8 | ~1.1 GB | ⏳ Should work |
| Phi-3-mini 3.8B | 7.6 GB FP16 | Int8 | ~3.8 GB | ⚠️ Tight fit, needs testing |

**Fast models** (<=3B) go on GPU. **Smart models** (7B+) run on CPU via
llama.cpp/candle when deeper reasoning is needed.

### Model Assignment (Realistic for Local Hardware)

| Profile | Model | Strategy | Temperature | Best At |
|---|---|---|---|---|
| `storyteller/fast` | RWKV 2.9B (GPU) | FastIterative | 0.6 | Prose, storytelling, chat |
| `coder/fast` | Qwen2.5-Coder 1.5B (GPU) | StructuredOutput | 0.1 | Code generation |
| `coder/review` | TinyLlama 1.1B (GPU) | StepByStep | 0.1 | Quick code review |
| `orchestrator/cpu` | 7B CPU model | ChainOfThought | 0.2 | Planning, decomposition |
| `assistant/fast` | RWKV 2.9B (GPU) | FastIterative | 0.4 | Chat, quick answers |
| `meta/theorist` | RWKV 2.9B (GPU) | Debate | 0.5 | Brainstorming, what-if |
| `meta/critic` | RWKV 2.9B (GPU) | StepByStep | 0.2 | Logical critique |

### NVIDIA Free API (Optional Supplement)

Only **`minimaxai/minimax-m3`** is reliably free on build.nvidia.com.
Other models (qwen, nemotron, glm) rotate in and out of free tier.
To discover currently available free models:

```bash
curl -s 'https://integrate.api.nvidia.com/v1/models' | \
  jq '.data[] | select(.id | test("free|community")) | .id'
```

NVIDIA's free tier: https://build.nvidia.com/explore/discover

---

## Unified Inference System (`inference/`)

The goal: **load anything, run anywhere**. A single trait + registry that
combines multiple backends:

| Backend | Model Types | Runtime | Status |
|---|---|---|---|
| web-rwkv | RWKV (RNN) | WGPU (Rust) | ✅ Working |
| candle | FFN Transformers (safetensors) | WGPU/CUDA (Rust) | 🚧 Stub |
| LiteRT | LLM, S2T, T2S, VLM, Diffusion, Embeddings | C++ (CPU/GPU/NPU) | 🚧 Stub |
| llama.cpp | GGUF models | C (CPU/GPU) | 🚧 Stub |
| whisper.cpp | Speech-to-text | C (CPU) | 🚧 Stub |

### Theoretical Performance Estimates (Before Testing)

Based on architecture analysis + literature:

| Model | Arch | Params | VRAM | Est. Load | Est. tok/s | Source |
|---|---|---|---|---|---|---|
| RWKV 2.9B (Int8) | RNN | 2.9B | 2.75 GB | 18s | 16-26 | 📊 Measured |
| MiniCPM5-1B (FP16) | FFN | 1.0B | 2.1 GB | 8-12s | 20-40 | 📐 Estimated (candle) |
| Qwen2.5-0.5B (FP16) | FFN | 0.5B | 1.0 GB | 5-8s | 30-60 | 📐 Estimated (candle) |
| TinyLlama-1.1B (Int8) | FFN | 1.1B | 1.1 GB | 3-5s | 20-35 | 📐 Estimated (llama.cpp) |
| SmolLM2-360M (FP16) | FFN | 0.36B | 720 MB | 2-4s | 10-25 | 📐 Estimated (LiteRT) |
| whisper-tiny | Speech | 0.039B | 150 MB | <1s | Real-time | 📐 Estimated (whisper.cpp) |
| embeddinggemma-300m | Embed | 0.3B | 600 MB | 2-3s | Batch | 📐 Estimated (LiteRT) |
| FLUX.2-klein-4B | Diff | 4.0B | 8 GB | 10-15s | 2-5 it/s | 📐 Estimated (LiteRT) |

### Bottleneck Analysis

The dominant load-time factor is **WGPU shader compilation** (~10-15s), not disk
I/O (NVMe reads 5.5GB in ~1.6s) or PCIe upload (2.75GB in ~0.5s). This means:
- First load is slow regardless of model size
- Keeping models warm (loaded) avoids re-compilation
- Subsequent loads of the same model are fast (~0.5-2s)
- The inference server (`roco-infer`) should keep a warm pool

### Load-Anything Strategy

1. **Auto-detect hardware**: GPU VRAM, CPU cores, RAM, SSD speed
2. **Estimate fit**: does model fit in VRAM? RAM? What quant needed?
3. **Pick best engine**: RWKV→web-rwkv, FFN→candle/LiteRT, S2T→whisper
4. **Load + cache**: keep warm for reuse, LRU eviction when full
5. **Route requests**: match task type to best loaded model

### Models Available via litert-community (HF)

| Category | Model | Size | Est. Load |
|---|---|---|---|
| 🎤 S2T | whisper-tiny | 75 MB | <1s |
| 🔊 T2S | parakeet-tdt-0.6b | 1.2 GB | 2-3s |
| 💬 LLM | SmolLM2-360M | 720 MB | 2-4s |
| 💬 LLM | Qwen2.5-0.5B | 1 GB | 3-5s |
| 💬 LLM | Qwen2.5-1.5B | 3 GB | 5-8s |
| 💬 LLM | Phi-4-mini | 7.6 GB | 10-15s |
| 👁 Vision | FastVLM-0.5B | 1 GB | 3-5s |
| 📐 Embedding | embeddinggemma-300m | 600 MB | 2-3s |
| 🤖 Function | functiongemma-270m | 540 MB | 2-3s |
| 🎨 Diffusion | FLUX.2-klein-4B | 8 GB | 10-15s |

**Total download**: ~25 GB across all categories

### Current Status

- [x] `InferenceEngine` trait + `BackendAdapter` for existing backends
- [x] `InferenceRegistry` — engine lifecycle, routing, queries
- [x] `HardwareCapabilities` — auto-detect GPU/CPU/RAM/SSD
- [x] `PerformanceProfile` — theoretical estimates from architecture + literature
- [x] `ModelEntry` + downloader — HF CLI integration for fetching models
- [x] 10 suggested litert-community models covering all categories
- [ ] Wire up candle backend for FFN transformers
- [ ] Wire up LiteRT backend for community models
- [ ] Wire up whisper.cpp for S2T
- [ ] Download and test actual models to replace estimates with measured data
- [ ] LRU eviction policy for warm model pool

---

## Infrastructure Needs

### FFN / Transformer Inference Engine

RWKV is an RNN — great for what it does, but we may need a separate **FFN transformer inference engine** that loads **SafeTensors** directly. Candidates:

- [`candle`](https://github.com/huggingface/candle) — Rust, supports SafeTensors, GPU via Metal/CUDA/WGPU
- [`llama.cpp`](https://github.com/ggerganov/llama.cpp) — GGUF format (convert from SafeTensors)
- [`mistral.rs`](https://github.com/EricLBuehler/mistral.rs) — Rust, supports ISQ quant, in-development
- [`burn`](https://github.com/tracel-ai/burn) — Rust, WGPU backend like web-rwvk
- Python subprocess — fallback, `transformers` + `safetensors` with `--model` arg

### PTH → SafeTensors Conversion

RWKV models ship as `.pth` (PyTorch). We have `scripts/pth_to_st_converter/`. Need a **general** converter that handles:

- RWKV .pth → SafeTensors ✅ (done)
- Other .pth files (e.g. older LLaMA derivations, custom checkpoints)
- Ideally: one script that reads any `.pth`, inspects keys, and writes SafeTensors with metadata

---

## Capability Tracker

> 0 / 25 capabilities implemented. Each is a potential agent role or pipeline.

### 🎨 Creative & Writing
- [ ] **Storytelling** — draft, expand, rewrite scenes with RWKV (fast iterations) + larger model critique
- [ ] **Writing companion** — real-time style suggestions, grammar, tone analysis
- [ ] **Companionship** — conversational agent with memory of user history

### 💻 Development
- [ ] **Coding** — multi-model: fast model for completions, smart model for architecture
- [ ] **Webdev** — generate components, debug CSS/JS, scaffold projects
- [ ] **Appdev** — generate Rust/TS boilerplate, review PRs, write tests
- [ ] **System monitoring** — tail logs, detect anomalies, suggest fixes

### 📊 Data & Scheduling
- [ ] **Log reader** — ingest logs, summarize errors, correlate with code changes
- [ ] **Scheduled tasks** — cron-like agent that runs checks, reports, scrapes
- [ ] **Organize user's messy files** — scan `~/Downloads`, `~/Desktop`, classify, move, tag

### 💼 Business & Marketing
- [ ] **Marketing** — generate copy, A/B test subject lines, analyze campaign performance
- [ ] **Fiverr & Upwork trends** — scrape, summarize what's in demand, suggest gigs
- [ ] **Devpost hackathon monitoring & strategizing** — find active hackathons, suggest projects
- [ ] **Job news** — monitor job boards, match against user's skills

### 🔬 Research & News
- [ ] **Stay up to date with news & science** — daily brief from RSS/APIs
- [ ] **Scan arxiv & research paper sites** — find papers matching user's interests, summarize
- [ ] **AI news** — track model releases, papers, industry moves
- [ ] **Bio news** — biotech, pharma, synthetic bio
- [ ] **South Africa news** — local news, policy, tech scene
- [ ] **Research topics of interest** — deep-dive into user's saved topics
- [ ] **AI research with Colab & Kaggle** — submit experiments, monitor training, analyze results
- [ ] **General research** — literature review, idea synthesis, cross-domain connections

### 🧠 Self-Reflection & Memory
- [ ] **Monitor own sessions** — categorize, tag, summarize each agent session
- [ ] **Memory agent** — persistent memory across sessions, recall relevant past context
- [ ] **User preemptive profiling from public information** — gather public data about user, build interest profile proactively

### 🔄 Meta & Trends
- [ ] **Theorizing with agents** — argue back and forth between models on a topic, record the debate
- [ ] **GitHub trends** — watch trending repos, summarize notable ones
- [ ] **Hacker News trends** — daily HN digest, surface hidden gems

---

## Agent Profiles (`agent_profile.rs`)

An agent is not just a model — it's a **role + model + system prompt + few-shot
+ strategy + state** bundle. Different foundation models behave radically
differently, so each needs its own strategy, not just a generic "agent".

### Architecture

```rust
pub struct AgentProfile {
    pub id: String,              // "orchestrator/smart", "coder/fast"
    pub name: String,
    pub role: AgentRole,         // Orchestrator | Worker | Verifier | Critic | Memory
    pub model_ref: String,       // key into model registry
    pub system_prompt: String,   // personality + instruction
    pub few_shot_examples: Vec<FewShotExample>,
    pub strategy: AgentStrategy, // FastIterative | StepByStep | StructuredOutput | CoT | Debate
    pub capabilities: Vec<String>,  // "code", "creative", "reasoning"
    pub weaknesses: Vec<String>,
    pub state: AgentState,       // conversation, memory, tokens used
}
```

### Key Insight: Different Models Need Different Strategies

| Profile | Model | Strategy | Temperature | Best At |
|---|---|---|---|---|
| `storyteller/fast` | RWKV 2.9B (GPU) | FastIterative | 0.6 | Creative prose, drafting |
| `coder/fast` | Qwen2.5-Coder 1.5B (GPU) | StructuredOutput | 0.1 | Code generation |
| `coder/review` | TinyLlama 1.1B (GPU) | StepByStep | 0.1 | Quick code review |
| `orchestrator/cpu` | 7B CPU model | ChainOfThought | 0.2 | Planning, decomposition |
| `assistant/fast` | RWKV 2.9B (GPU) | FastIterative | 0.4 | Chat, quick answers |
| `meta/theorist` | RWKV 2.9B (GPU) | Debate | 0.5 | Brainstorming, what-if |
| `meta/critic` | RWKV 2.9B (GPU) | StepByStep | 0.2 | Logical critique |

**Local-first**: all GPU models fit in 4GB VRAM with Int8 quant. No API calls
required. CPU model is optional for deep reasoning when GPU is occupied.

### Grouping & Routing

Profiles are organized into **groups** with a routing strategy:

```text
writing group (FirstAvailable):
  ├── storyteller/fast       ← uses this one

coding group (EscalateOnFailure):
  ├── coder/fast              ← try first (fast)
  └── coder/review            ← escalate if code quality fails (thorough)

meta group (Ensemble BestOfN):
  ├── meta/theorist           ← proposes
  └── meta/critic             ← critiques
```

### Fast Swap

- Profiles reference a `model_ref` (string key). Change the ref to swap
  foundation models without changing the prompt/strategy.
- Backends are attached/detached independently via `AgentProfileRegistry::attach_backend()`
  and `detach_backend()`.
- The `roco-infer` server handles the actual VRAM/RAM management — profiles
  just say which model they want.

### On-Device Fine-Tuning & LoRA (Future)

Each profile could eventually carry a **LoRA adapter** or **fine-tuned delta**
that gets applied on top of the base model:

```rust
pub struct ProfileLoRA {
    pub adapter_path: PathBuf,
    pub target_modules: Vec<String>,  // "q_proj", "v_proj", etc.
    pub scale: f32,
}
```

This would allow per-task specialization without duplicating the full model.

### Current Status

- [x] `AgentProfile` with role, model_ref, system prompt, few-shot, strategy, state
- [x] `AgentStrategy` variants: FastIterative, StepByStep, StructuredOutput, CoT, Debate, Escalate
- [x] `AgentGroup` with routing: FirstAvailable, Ensemble, EscalateOnFailure, ByCapability, RoundRobin
- [x] `AgentProfileRegistry` with profile lifecycle + backend attachment
- [x] 7 built-in presets — all local-first (storyteller/rwkv, coder/qwen, reviewer/tinyllama, orchestrator/cpu, assistant/rwkv, theorist/rwkv, critic/rwkv)
- [x] JSON serialization for save/load from config files
- [x] Local-first ethos: no API-reliant presets. All GPU models fit in 4GB VRAM with Int8 quant
- [ ] Wire profiles into `Orchestrator` (use profile's strategy settings)
- [ ] Wire profiles into `roco-infer` (profile → model → backend mapping)
- [ ] Profile hot-reload from config changes
- [ ] LoRA/fine-tune adapter support

---

## Inference Server (`roco-infer`)

A background daemon that manages model lifecycle, RAM/VRAM, and serves an OpenAI-compatible API.

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    roco-infer                           │
│                                                         │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐  │
│  │  Model   │  │  Model   │  │   Memory Manager     │  │
│  │ Registry │  │  Loader  │  │  (RAM / VRAM budget) │  │
│  └──────────┘  └──────────┘  └──────────────────────┘  │
│         │              │               │                │
│         ▼              ▼               ▼                │
│  ┌──────────────────────────────────────────────────┐   │
│  │           HTTP API (axum)                        │   │
│  │  GET  /health                                    │   │
│  │  GET  /v1/models          — list loaded models   │   │
│  │  POST /v1/models/load    — load a model          │   │
│  │  POST /v1/models/unload  — unload a model        │   │
│  │  POST /v1/completions    — generate              │   │
│  │  POST /v1/chat/completions — chat completions    │   │
│  │  GET  /v1/memory         — memory usage report   │   │
│  └──────────────────────────────────────────────────┘   │
│                                                         │
│  Lock file protocol: /tmp/roco-infer/<model-hash>.lock  │
│  Removed on process stop. Stale locks cleaned on boot.  │
└─────────────────────────────────────────────────────────┘
```

### Lock File Protocol

```
/tmp/roco-infer/
  ├── rwkv7-2b9.st.lock       # pid=12345
  ├── qwen2.5-coder-1.5b.st.lock  # pid=12345
  └── deepseek-coder-6.7b.gguf.lock # pid=23456
```

- Each model gets a `.lock` file named after its model hash/filename.
- Content: `pid=<process_id>`.
- On startup: scan lock dir, check each PID with `kill(pid, 0)` (Unix).
  If process is dead, remove the stale lock and load the model.
- On shutdown: remove all lock files owned by this process.
- On crash: stale locks remain, cleaned on next startup.
- If two processes want the same model: second one sees the lock and either
  queues (if config says shared) or errors (if config says exclusive).

### Memory Management

| Setting | Flag | Behavior |
|---|---|---|
| Max VRAM | `--max-vram-gb` | Auto-detect GPU VRAM, refuse loads that exceed budget |
| Max RAM | `--max-ram-gb` | Soft limit; warns when exceeded |
| Auto-unload | (future) | Least-recently-used model evicted when budget exceeded |

### Current Status

- [x] Crate scaffold: `crates/infer/` with axum server
- [x] Endpoints: health, list/load/unload models, completions, memory report
- [x] Lock file protocol with stale cleanup
- [x] CLI args for binding, pre-load, limits
- [ ] Wire up real model loading (mock backend placeholder currently)
- [ ] GPU VRAM detection on startup
- [ ] Auto-unload LRU policy
- [ ] Shared model access between processes

---

## Eval Framework (`eval_suite`)

Standalone model evaluation that tests backends **directly** (not through the
orchestrator pipeline). Runs configurable test cases and produces structured
JSON reports.

### Why not the existing `eval.rs`?

The existing `eval.rs` runs evals through the full orchestrator + verifier
pipeline. That's useful for integration testing but slow and requires a fully
configured agent loop. `eval_suite` tests the bare model backend on specific
capabilities: instruction following, coherence, repetition, format compliance.

### Eval Categories

| Category | What it tests | Example case |
|---|---|---|
| Smoke | Does the backend respond at all? | "Say 'hello'" |
| Instruction | Does the model follow constraints? | Multi-step instruction, negative instruction |
| Coherence | Is the output sensible and on-topic? | Explain a concept, write a story |
| Repetition | Does the model loop? | List 5 items, check for unique sentences |
| Throughput | Tokens/second | Generate 512 tokens, measure wall time |
| Format | Output structure compliance | JSON, numbered lists |
| Context | Long input handling | Answer question about a long passage |

### Running

```bash
# Mock backend (no model needed)
cargo run --example eval_suite

# Local RWKV
cargo run --example eval_suite --release -- --backend rwkv

# NVIDIA API
cargo run --example eval_suite -- --backend nvidia --filter coherence

# JSON report
cargo run --example eval_suite -- --output evals/results/latest.json
```

### Current Status

- [x] `eval_suite.rs` module in `roco-core`
- [x] 12 built-in eval cases across all categories
- [x] JSON report writer
- [x] Human-readable summary printer
- [x] Example runner binary with CLI
- [ ] Per-model expected scores (calibrate against known-good models)
- [ ] Visual diff of output vs expected
- [ ] Regression tracking over time
- [ ] Integration with CI

---

## Open Questions / Blockers

- [ ] Cleanup segfault at exit (wgpu resource drop ordering)
- [ ] Need a general `.pth` → SafeTensors converter for non-RWKV models
- [ ] Need a second inference engine for FFN transformers (candle? llama.cpp? mistral.rs?)
- [ ] How to route tasks between fast (GPU) and smart (CPU) models — orchestration layer
- [ ] Lock file protocol: what if two processes both want the same model? Priority / queue?
- [ ] Critique loop: model A generates, model B critiques, model A revises — cost vs. quality tradeoff
- [ ] Multi-model orchestrator: how to decompose tasks and dispatch subtasks to the right model
- [ ] Memory format: what does cross-session memory look like? Vector store? Structured logs? SQLite?
- [ ] Scraping infra: RSS readers, API wrappers, browser automation for trends/news monitoring
- [ ] Scheduled task engine: cron-like agent loop that persists and executes on schedule
- [ ] `roco-infer`: wire up real model loading (RwkvBackend, candle, llama.cpp subprocess)
- [ ] `eval_suite`: calibrate expected scores per model, add regression tracking
