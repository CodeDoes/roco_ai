# RoCo AI

AI-assisted collaborative writing tool powered by a local LLM (RWKV-7 2.9B).

## Quick Start

```bash
# Generate story from premise
./start.sh "A lighthouse keeper discovers a hidden message in the fog"

# Desktop GUI
./run_desktop.sh

# Run full test suite
./run_tests.sh

# Live codebase introspection
./scout.sh
```

## Key Commands

- `./scout.sh` - Codebase introspection (crates, deps, line counts, duplicate detection)
- `roco gui` - Desktop GUI
- `roco interact` - Interactive chat
- `roco server` - HTTP server for editor plugins
- `./docs_helper.sh list` - Documentation navigation helper

---

## Crate Inventory & Test Map (Condensed from `USE_CASES_AND_GAPS.md`)

RoCo AI consists of 19 workspace crates organized as follows:

| Category | Crates | Description |
|---|---|---|
| **Production Agents** | `agent` | Story generation pipelines, ReAct agent loop, outline revision, pacing & quality steering |
| **App & State** | `app`, `core`, `session`, `workspace` | AppContext binding, session store, persistent timeline, sandboxed file workspace |
| **LLM & Grammar** | `inference`, `inferd`, `infer-client`, `engine`, `grammar`, `bnf-engine` | `web-rwkv` GPU thread, inference daemon, token-level kbnf BNF grammar decoder |
| **Transport & Server** | `server`, `gateway`, `cli`, `chat-common`, `message`, `protocol` | HTTP server, gateway routing, CLI commands, shared chat protocol types |
| **UI & Validation** | `ui`, `validation`, `tools` | Desktop widgets (editor, link graph, pacing), multi-layer verifiers, file/execution tools |

### Existing Tests & Test Coverage Goals
- **Unit & Integration Test Coverage:** `crates/app/tests/facade.rs`, `crates/engine/src/tests/eval_suite.rs`, `crates/ui/tests/token0_probe.rs`, unit tests in `session`, `workspace`, `grammar`, `message`, `validation`, `chat-common`, `gateway`, `tools`.
- **Primary Coverage Gaps to Fill:** Expanding integration test suites for `cli`, `ui`, `server`, and full-stack session persistence under high concurrency.

---

## Key Architecture Specs & Useful RFC Takeaways

- **Harness Architecture (RFC 0001 / 0002 / 0004):**
  - Execution environments are separated from model weights via `DomainHarness` (`name`, `init`, `run`, `verify`, `rollback`).
  - Retries up to `max_retries` (default 3) on verification failure. Harness engineering provides +15–25% accuracy improvements over raw prompt tweaks.
- **Security Boundaries (RFC 0006):**
  - All workspace file access passes through `Sandbox` with `is_safe_relative_path()` (blocks absolute paths, prefix escapes, and `..` relative traversals).
  - 10MB file limit; extension whitelist (`txt`, `md`, `json`, `py`, `rs`). Air-gapped: zero outbound network calls in execution loops.
- **Stuck-State Detection & Rollbacks (RFC 0007 / 0014):**
  - Failures in `Verifier::verify()` increment state attempt counters and invoke `rollback()`.
  - Exceeding `max_retries` exits execution cleanly with `StackResult.success = false` and records `rollback_count` without process crashes.
- **Offline RWKV Protocol (RFC 0008 / 0015):**
  - Production uses local `.st` weights via `roco-inferd` / `web-rwkv` WGPU thread with `llvmpipe` CPU fallback.
  - Generative decoding uses token-level kbnf BNF grammar constraints (`strict_grammar = true`) to physically prevent hallucinated tokens or raw formatting leakages. Zero cloud fallback.
- **Air-Gapped Local Memory (RFC 0010):**
  - Persistent state saved only under local `.roco/` directories. User consent required per entry via `RecallTool` / `RememberTool`.

---

## Architectural Goal: Resolving `SIMPLICITY_AND_SAFETY_DEEP_DIVE.md`

A primary ongoing engineering objective is to **resolve the issues identified in `docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md`**, making that document historical and less relevant over time:

1. **Crate Consolidation:** Consolidate the 19 workspace crates down to 6–8 unified crates:
   - Merge `message`, `chat-common`, `protocol` → `roco-protocol`
   - Merge `agent`, `validation`, `tools` → `roco-agent`
   - Consolidate `engine`, `inference`, `grammar` → `roco-engine`
2. **Harness Isolation:** Move the `local_agent` mock framework out of `crates/app/src/` into a dedicated package (`crates/roco-harness` or integration tests) to separate live production app logic from mock evaluation scaffolds.
3. **Linker & Build Portability:** Add fallback checks for developer systems lacking `mold` so `cargo build` succeeds seamlessly out of the box.
4. **Unified Workspace Sandboxing:** Standardize workspace path containment and timeline management across GUI, CLI, and server interfaces.

---

## Documentation

- `./USE_CASES_AND_GAPS.md` - Detailed breakdown of crate dependencies, test suites, and coverage gaps.
- `./docs/SIMPLICITY_AND_SAFETY_DEEP_DIVE.md` - System anatomy, DX analysis, safety guarantees, and simplification goals.
- `./docs/rfc/` - Directory of micro-RFCs containing hyper-specific technical specifications:
  - `0001-local-ai-harness.md` - Local AI Harness Architecture
  - `0004-harness-vs-fine-tuning-deep-dive.md` - Harness engineering vs weight updates
  - `0006-security-boundary-model.md` - Security boundary & path containment
  - `0007-rollback-detection-algorithm.md` - Stuck-state detection & rollback algorithm
  - `0008-offline-inference-protocol.md` - Offline inference protocol for edge deployment
  - `0010-privacy-preserving-rag.md` - Air-gapped local context memory

---

## Environment & Notes

- **Model:** RWKV-7 2.9B (set `RWKV_MODEL` or auto-detected in `./models`)
- **Config:** `.roco/config.toml` or `~/.config/roco/config.toml`
- **Rust:** Edition 2021
- **GPU Debug:** For debug GPU hangs, set `RWKV_ADAPTER=llvmpipe`
