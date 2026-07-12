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

### Why not just RWKV?

RWKV is great (fast, small footprint, local), but we may need:

| Need | Model Type | Location |
|---|---|---|
| Fast generation, low latency | RWKV, Phi, TinyLlama, Qwen2.5-Coder-0.5B | `~/Documents/models/` (fast models) |
| Deep reasoning, planning | Larger Llama, Qwen, DeepSeek (CPU offload) | CPU inference, slower but smarter |
| Code-specific | DeepSeek-Coder, StarCoder, Qwen2.5-Coder | GPU if fits, CPU otherwise |
| Research / long context | Gemma-2, Mistral, Yi-34B | CPU or multi-GPU |

**Fast models** (<=3B params) go on GPU for interactive tasks. **Smart models** (7B+) run on CPU if GPU VRAM is limited — they direct the fast models via critique + task decomposition.

### Critiquing Output & Model Assignment

Each model's output should be critiqued (by a second model or heuristic), and models should be assigned to tasks they're strong at:

- **RWKV 2.9B**: fast prose, storytelling drafts, chat, system monitor narration. Weak at: deep coding, math, long-range coherence.
- **Phi-3 / Phi-3.5**: good at reasoning for its size, coding, structured output. Weak at: creative writing (too stiff).
- **Qwen2.5-Coder 1.5B**: decent code completion, fast. Weak at: anything non-code.
- **DeepSeek-Coder 6.7B** (CPU): proper code generation, refactoring. Weak at: latency.
- **Llama-3 8B** (CPU): general reasoning, planning, orchestration. Weak at: speed.

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
