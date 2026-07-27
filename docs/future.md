# Future Work — UX, Modularity, Determinism, Interpretability

This document catalogs concrete improvements beyond the current state, organized by the four pillars. Each item includes rationale and a suggested approach.

---

## UX — User Experience

### 1. Interactive `roco inspect` with live backend connection
**Rationale:** `roco inspect` currently shows static config and disk state. It cannot inspect the live model's recurrent state, active sessions in the state pool, or the GPU memory footprint.

**Approach:**
- Add `roco inspect live` that connects to a running backend (inferd or direct) and queries:
  - Active session keys and their LRU position
  - Current GPU memory usage (`wgpu` adapter queries)
  - Model metadata (loaded layers, quant scheme, context window)
  - Sampling parameters in use (temperature, top_p, seed)
- Output as structured JSON for scripting

### 2. Generation progress bar with ETA
**Rationale:** Long generations (story chapters, multi-turn reasoning) give no feedback besides the first token appearing. Users don't know if the model is still processing the prompt, sampling, or stuck.

**Approach:**
- Add `--progress` flag to `roco interact`, `roco story`
- During prompt processing: show `Processing prompt... [tok/s]`
- During generation: show `Generating [tok/s] | tokens: N/M | est. remaining: Xs`
- The actor thread already reports `tokens generated` — pipe this through a channel to the UI thread, or poll periodically via a new `ActorMessage::Progress` message

---

## Modularity — Crate Architecture & Separation of Concerns

### 6. Consolidate 16 crates → 7 core crates (revisit RFC 0001)
**Rationale:** The workspace has 16 crates (down from 19). Cross-crate changes still require touching 3–5 crates. Build times suffer from the compilation unit boundary overhead.

**Proposed consolidation (builds on `AGENTS.md`):**

| Current Crates | Proposed Crate | Rationale |
|---|---|---|
| `message`, `chat-common`, `protocol` | `roco-protocol` | All define wire types; changing one field touches all three |
| `agent`, `validation`, `tools` | `roco-agent` | Agent loop, verifiers, and tool registry are tightly coupled |
| `engine`, `inference`, `grammar`, `bnf-engine` | `roco-engine` | ModelBackend trait + backends + grammar all in one compilation unit eliminates the `Box<dyn BnfMask>` dance |
| `session`, `workspace` | `roco-store` | Both are persistence-layer concerns with overlapping path logic |
| `gateway`, `server`, `infer-client` | `roco-net` | Transport layer — all HTTP, all share route types |
| `cli`, `app` | `roco-cli` (keep) | Already the primary binary |
| `ui` | `roco-ui` (keep) | Desktop GUI is self-contained |
| `harness` | (keep standalone or merge into test infrastructure) | Evaluation harness is a dev-dependency |

**Estimated impact:**
- Reduction from 16 → 7 workspace crates
- One `cargo check` compiles ~40% fewer crate boundaries
- Changes to wire types touch exactly 1 crate instead of 3

---

### 9. Decouple the gateway's daemon lifecycle from the CLI
**Rationale:** `roco gateway start` manages daemon lifecycle (start inferd, health-check, restart on crash). This logic lives in the CLI binary, making it impossible to use the gateway as a library without daemon-management side effects.

**Approach:**
- Extract `DaemonManager` into its own crate (`roco-daemon` or within `roco-net`)
- The CLI calls `DaemonManager::start("inferd", args)` and `DaemonManager::start("gateway", args)`
- Library users can opt out of daemon management entirely

---

## Determinism — Reproducible & Predictable Behaviour

### 13. Deterministic test fixtures for the eval suite
**Rationale:** `roco eval-suite` runs deterministic checks but doesn't test model-level determinism (same seed → same output from the real backend).

**Approach:**
- Add `eval-suite` test: "deterministic_seed_model" — sends the same `CompletionRequest` with `seed=42` twice to a running backend, asserts `response_a.text == response_b.text`
- Run this test only when a real backend is available (skip on `MockBackend`)
- CI can run this against the production inferd if available

---

## Interpretability — Understanding What the Model Is Doing

### 14. Token-level trace logging
**Rationale:** When a generation goes wrong (repeats, hallucinates, goes off-topic), there's no way to see what tokens were sampled, what the probabilities were, or where the grammar intervened.

**Approach:**
- Add a `token_trace: Vec<TokenTrace>` field to `CompletionResponse`
- `TokenTrace` contains: `token_id`, `token_str`, `probability`, `temperature`, `top_p_cut`, `grammar_masked: bool`, `selected_by_grammar: bool`
- Enable with `CompletionRequest::record_trace(true)` or `--trace` CLI flag
- Display via `roco inspect trace --session <id> --last` or a new `roco trace` subcommand
- Conditional compilation (`#[cfg(feature = "trace")]`) so release builds don't pay the memory cost

### 15. Token probability heatmap in the TUI/GUI
**Rationale:** The desktop GUI (`crates/ui`) has no way to show uncertainty. Users see the final text but not the model's confidence.

**Approach:**
- Add a "confidence" color overlay to the markdown editor: green (p > 0.9), yellow (0.5-0.9), red (< 0.5)
- Show per-token probability on hover
- Requires the trace data from item 14

### 16. Generative debug REPL
**Rationale:** Debugging a bad generation requires setting breakpoints in `handle_complete` — a 900-line async function on a separate OS thread. There's no way to step through generation interactively.

**Approach:**
- Add `roco debug` subcommand that starts a generation in single-step mode:
  - After each token, print: `token=1234 "the" p=0.87 top5=[(1234,0.87), (5678,0.05), ...]`
  - Press Enter to advance one token, or type `continue` to finish
  - Type `state` to dump recurrent state statistics (mean activation, variance)
  - Type `grammar` to show current grammar stack
- Implement via a new `ActorMessage::Step` variant that the actor processes

### 17. Generation health metrics dashboard
**Rationale:** Operators and developers need visibility into live generation health without reading log files.

**Approach:**
- Add `roco inspect metrics` that exposes:
  - Tokens per second (instant + rolling average)
  - GPU utilization (from `wgpu` adapter queries)
  - State pool size / eviction rate
  - Cache hit ratio
  - Number of cancelled/interrupted generations
- Output as JSON for Prometheus/Grafana ingestion
- Optional: push metrics to a statsd endpoint

### 18. Structured logging for the generation pipeline
**Rationale:** Current tracing is ad-hoc `info!` calls in `actor.rs`. There's no structured span hierarchy linking a completion request through its entire lifecycle.

**Approach:**
- Add `tracing` spans with fields:
  - `ot.name = "handle_complete"`, `req.prompt_len`, `req.seed`, `req.temperature`
  - Child span `sampling` with `token_id`, `prob`, `selected_by_grammar`
  - Child span `token_decode` with `word`, `word_len`
- Export via `tracing-subscriber` to `OTLP` or JSON file
- Add `ROCO_TRACE=1` env var to enable detailed per-token tracing to stderr

### 19. Session state visualization
**Rationale:** The recurrent state is an opaque tensor. There's no way to see how it evolves across turns — which dimensions activate, how blend operations reshape it, etc.

**Approach:**
- Add `roco inspect state --session <id>` that:
  - Loads the session's recurrent state from the pool
  - Computes summary statistics: mean, std, min, max per layer
  - Computes per-layer entropy (how "surprised" is the model?)
  - Outputs as JSON for external visualization (e.g., `python -c "import json, matplotlib; ..."`)
- Add a lightweight TUI view in `crates/ui` that plots the state distribution as an ASCII histogram

---

## Testing & CI

### 20. Real integration tests for the full pipeline
**Rationale:** The eval suite tests components in isolation. There's no end-to-end test that starts inferd, sends a request, and verifies the response.

**Approach:**
- Add `tests/pipeline_test.rs` that:
  - Starts `roco-inferd` (or connects to an already-running one)
  - Sends a `CompletionRequest` with known parameters
  - Verifies the response is well-formed JSON
  - Verifies deterministic seed produces identical results across two calls
- Guard with `#[cfg(feature = "integration")]` so it doesn't run in regular `cargo test`
- Add to CI as a separate job

### 21. Fuzz testing for the grammar engine
**Rationale:** The bnf-engine/kbnf grammar parser is complex. Malformed grammars could panic or hang.

**Approach:**
- Add `cargo fuzz` targets for:
  - Parsing random GBNF strings (ensure no crash)
  - Masking random logits with random grammar states (ensure no panic)
  - Accepting random token sequences (ensure no infinite loop)
- Run in CI with a short timeout


---

## Story Pipeline Fault Tolerance & Pipeline Safety



---

## Build & Developer Experience


### 25. Cargo workspace hygiene
**Rationale:** Several crates have unused dependencies or mismatched feature flags.

**Approach:**
- Run `cargo machete` (unused dependency detector) regularly
- Add `cargo deny` to CI for license checks, duplicate crate detection, and security advisories
- Unify `tokio` feature flags across all crates (some enable `process`, others don't)
- Add a workspace-level `[lints.rust]` section for consistent clippy configuration

---

## Summary of Priorities

| Priority | Area | Item | Effort | Impact |
|----------|------|------|--------|--------|
| P1 | Modularity | 6. Crate consolidation | 3-5 days | High — halves build time, simplifies navigation |
| P1 | Interpretability | 14. Token trace logging | 2 days | Medium — enables debugging bad generations |
| P2 | UX | 2. Progress bar | 1 day | Medium — better feedback for long gen |
| P3 | Interpretability | 16. Debug REPL | 3 days | Low — power-user tooling |
| P3 | Testing | 21. Fuzz grammar engine | 2 days | Low — security hardening |
