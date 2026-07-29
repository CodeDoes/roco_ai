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

## Crate Inventory & Test Map

RoCo AI consists of 18 workspace crates organized as follows:

| Category | Crates | Description |
|---|---|---|
| **Agents & Tools** | `agent` | Story generation pipelines, ReAct agent loop, outline revision, pacing & quality steering |
| **App & State** | `app`, `session`, `workspace` | AppContext binding, session store, persistent timeline, sandboxed file workspace |
| **LLM Engine** | `engine`, `engine-gpu` | `engine`: trait + types + grammar + mock. `engine-gpu`: RwkvBackend (web-rwkv/WGPU) |
| **Inference** | `inferd`, `infer-client` | Inference daemon, remote inference client |
| **Transport & Server** | `server`, `gateway`, `cli`, `protocol` | HTTP server, gateway (unified API), CLI commands, shared chat protocol types |
| **UI** | `ui` | Desktop widgets (editor, link graph, pacing) |
| **Testing** | `harness` | Mock execution framework for offline testing |
| **GPU Backends** | `rwkv7-vulkan` | Vulkan compute backend (optional) |

### Architecture: Engine vs Engine-GPU

The LLM engine is split into two crates for dependency isolation:

- **`roco-engine`** (14 dependent crates): Trait definitions (`ModelBackend`), types (`CompletionRequest`/`CompletionResponse`), BNF grammar engine, mock backend. No GPU dependency.
- **`roco-engine-gpu`** (2 dependent crates): `RwkvBackend` implementation using web-rwkv/WGPU. Only compiled when real inference is needed.

This separation keeps build times fast for most crates while isolating GPU dependencies.

### Gateway: Unified HTTP API

The gateway (`roco-gateway`) is the unified HTTP API endpoint with three deployment modes:

1. **Full mode**: Gateway + local backend (no inferd needed)
2. **Proxy mode**: Gateway + inferd (existing behavior)
3. **Workspace-only mode**: Gateway + no backend (session/workspace management)

Gateway routes include:
- Direct inference: `/complete`, `/bake`, `/vocab`, `/v1/completions`
- Sessions: CRUD, bake, generate, streaming
- Workspaces: CRUD, file operations
- Jobs: create, stream, cancel

The `server` crate remains for standalone `inferd` daemon use case.

### Existing Tests
- **Unit tests**: 229+ tests pass across all crates
- **Integration tests**: `app/tests/facade.rs`, `engine/src/tests/eval_suite.rs`, `ui/tests/`
- **Primary gaps**: `cli`, `ui`, full-stack session persistence under high concurrency

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

## Architecture Status

### Completed
- ✅ **Engine split**: `engine` (trait/types/mock) + `engine-gpu` (RwkvBackend) — clear dependency separation
- ✅ **Gateway unification**: Single HTTP API with optional backend support (local/proxy/workspace-only modes)
- ✅ **bnf-engine removed**: 57-line re-export wrapper eliminated
- ✅ **Session workspace switching**: `Session::set_workspace()` method added
- ✅ **Build portability**: `check_mold.sh` script for linker verification

### Remaining Work
- **Crate consolidation**: Merge `protocol` modules, consolidate `agent` with `validation`/`tools`
- **Deduplicate types**: Remove duplicate `Agent`, `BakeRequest`/`BakeResponse`, etc. across crates
- **Harness cleanup**: Replace 10 identical `Agent` structs with generic `MockAgent`
- **Placeholder tests**: Replace stub test modules with real tests
- **TODO resolution**: Complete unfinished story route handlers in CLI

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
