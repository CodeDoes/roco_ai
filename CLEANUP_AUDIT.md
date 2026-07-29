# RoCo AI — Project Health Audit & Cleanup Plan
**Date:** 2026-07-29  
**Status:** Build ✓ | Tests ✓ (229 pass) | Clippy ✗ (toolchain mismatch)

---

## Executive Summary

The project is functional but structurally bloated. We have **19 crates** where 6–8 would suffice, duplicate type definitions across the workspace, a 57-line wrapper crate (`bnf-engine`) that exists solely to re-export another crate, and a mock harness with 10 identical `Agent` structs. The tmux emulator (2,179 lines Python) from upstream is integrated but unused by the Rust codebase.

**Recommended action:** Consolidate crates, remove dead code, and fix the mold linker portability issue. This will reduce compilation time, cognitive load, and the surface area for bugs.

---

## Current State

### Build & Test Health
| Metric | Status |
|--------|--------|
| `cargo check` | ✅ Compiles clean |
| `cargo test` | ✅ 229 unit tests pass, 0 failures |
| `cargo clippy` | ❌ Nix/rustup toolchain mismatch (known issue) |
| Warnings | 0 (clean) |
| TODOs/FIXMEs | 16 in production code |

### Crate Inventory (19 crates, 74,602 lines)

| Crate | Lines | Tests | In Workspace | Notes |
|-------|-------|-------|--------------|-------|
| agent | 20,268 | 245 | ✅ | Largest crate — story pipelines, ReAct, validation, tools |
| cli | 15,631 | 177 | ✅ | CLI commands, depends on 11 workspace crates |
| ui | 9,417 | 205 | ✅ | Desktop widgets (egui) |
| engine | 9,019 | 147 | ✅ | ModelBackend trait, eval framework, grammar |
| inference | 6,481 | 20 | ✅ | web-rwkv backend, quantization, sampling |
| app | 2,681 | 50 | ✅ | AppContext, session binding, workspace timeline |
| protocol | 1,962 | 56 | ✅ | Chat protocol types, formatting |
| gateway | 1,762 | 9 | ✅ | HTTP gateway/router |
| server | 1,402 | 10 | ✅ | HTTP server, routes |
| workspace | 1,207 | 23 | ✅ | Sandbox, file access, version control |
| harness | 1,076 | 29 | ✅ | Mock execution framework (10 duplicate Agents) |
| core | 982 | 6 | ❌ | Shared types/traits — NOT IN WORKSPACE |
| session | 770 | 5 | ✅ | Persistent session store |
| rwkv7-vulkan | 655 | 0 | ✅ | Vulkan compute backend |
| infer-client | 701 | 13 | ✅ | Remote inference client |
| rwkv7-opencl | 384 | 2 | ❌ | OpenCL backend — NOT IN WORKSPACE |
| inferd | 116 | 0 | ✅ | Inference daemon (thin wrapper) |
| bnf-engine | 57 | 3 | ✅ | 57-line re-export of engine::bnf_engine |
| rwkv7-triton | 31 | 2 | ❌ | Triton kernels — NOT IN WORKSPACE |

### Dependency Graph (most depended-on)
```
roco-engine       (14 dependents) — foundational
roco-protocol      (6 dependents)
roco-session       (5 dependents)
roco-workspace     (5 dependents)
roco-bnf-engine    (5 dependents) — should be merged into engine
roco-app           (4 dependents)
roco-infer-client  (3 dependents)
roco-agent         (3 dependents)
```

### High Fan-Out (most dependencies)
```
roco-cli: 11 workspace crate dependencies
```

---

## Problems Found

### 1. Crate Fragmentation (19 → should be 6–8)

**The `bnf-engine` wrapper is pure noise:**
```rust
// crates/bnf-engine/src/lib.rs — 57 lines total
pub use roco_engine::bnf_engine::*;
```
This crate exists only to re-export. 5 other crates depend on it, creating an unnecessary indirection layer.

**`core` is orphaned:**
The `core` crate (982 lines) defines shared types (`ModelBackend`, `RoCoError`, `Tool`, `Runtime`) but isn't in the workspace. It's either dead or was intended to be the consolidation target but never wired in.

**Proposed consolidation:**
| Before | After | Rationale |
|--------|-------|-----------|
| `engine` + `bnf-engine` + `inference` | `roco-engine` | Grammar, backend trait, and inference are one concern |
| `protocol` + `chat-common` (if exists) | `roco-protocol` | Already mostly unified |
| `agent` + `validation` + `tools` | `roco-agent` | Tools and validation are agent internals |
| `session` + `workspace` | `roco-session` | Both manage persistent state |
| `app` stays | `roco-app` | Thin binding layer |
| `cli` stays | `roco-cli` | Binary target |
| `ui` stays | `roco_ui` | Desktop GUI |
| `server` + `gateway` | `roco-server` | HTTP routing is one concern |
| `harness` | `roco-harness` | Mock framework (stays separate) |
| `rwkv7-*` | `rwkv7-backend` or keep separate | GPU backends (keep modular) |

### 2. Duplicate Types Across Workspace

The `scout.sh dupes` output reveals significant type duplication:

| Type | Declarations | Location |
|------|-------------|----------|
| `Agent` | 11 | `agent/src/common_agent.rs` + 10 harness domain files |
| `MockBackend` | 3 | Multiple locations |
| `BakeResponse` / `BakeRequest` | 3 each | protocol, session, core |
| `Workspace` | 2 | workspace, core |
| `ValidationSeverity` | 2 | agent/validation, core |
| `Tool` / `ToolError` | 2 each | agent/tools, core |
| `SessionStore/Handle/Status/Error` | 2 each | session, core |
| `ServerConfig` | 2 | server, core |
| `StrategySelector/Kind` | 2 | engine, core |
| `QualityIssue` | 2 | agent, core |

**Root cause:** The `core` crate re-exports types that are also defined in their "home" crates. This creates confusion about which is canonical.

### 3. Harness Duplicate Agents

`crates/harness/src/` contains10 domain files (`pet.rs`, `email.rs`, `html.rs`, etc.) each defining an identical `Agent` struct:

```rust
pub struct Agent;
impl DomainHarness for Agent {
    fn name(&self) -> &'static str { "pet" }  // only this differs
    fn run(&self, input: &str, ctx: &Context) -> Result<String, HarnessError> {
        Ok(MockBackend.generate(&format!("{} ctx={:?}", input, ctx.session_id)))
    }
    // ... identical impl
}
```

These could be a single generic `MockAgent { domain: &'static str }` struct.

### 4. Placeholder Test Files

5 test module files are byte-identical:
```
crates/agent/src/validation/tests/mod.rs
crates/agent/src/tools/tests/mod.rs
crates/protocol/src/message/tests/mod.rs
crates/protocol/src/chat_common/tests/mod.rs
crates/engine/src/grammar/tests/mod.rs
```

These are likely stubs that should either contain real tests or be removed.

### 5. Mold Linker Portability

`.cargo/config.toml` defaults to `mold` linker. Systems without `mold` installed will fail to build with no clear error message. Need a fallback.

### 6. TODO Debt (16 items)

Key unfinished items:
- `gateway/src/lib.rs:701` — `active_jobs` filtering is stubbed
- `cli/src/story_routes.rs` — 6 story route handlers are TODO stubs
- `inference/examples/story_*.rs` — 3 story pipeline examples have unimplemented features

### 7. Python Tmux Emulator (2,179 lines)

Upstream added `scripts/tmux_roco/` — a Python mirror of the Rust harness. It's self-contained and doesn't interact with the Rust codebase. This is useful for testing but adds maintenance burden.

---

## Recommended Cleanup Plan

### Phase 1: Quick Wins (1–2 hours)

1. **Delete `crates/bnf-engine/`** — re-export wrapper adds no value
   - Update 5 dependent crates to use `roco-engine` directly
   - Remove from workspace members

2. **Wire `core` into workspace** OR **merge it into `engine`**
   - If `core` is intended as the shared types crate, add it to workspace
   - If it's dead, delete it and remove duplicate type definitions

3. **Fix mold linker portability**
   - Add a `build.rs` check or document `RUSTFLAGS=""` fallback
   - Or switch to `lld` as default with mold as optional

4. **Add `rwkv7-opencl` and `rwkv7-triton` to workspace members**
   - They're compiled but not tracked

5. **Deduplicate harness Agent structs**
   - Replace 10 identical files with a single generic `MockAgent`

### Phase 2: Consolidation (4–8 hours)

1. **Merge `bnf-engine` into `engine`** (Phase 1 may handle this)
2. **Merge `inference` into `engine`** — they're tightly coupled
3. **Merge `server` + `gateway`** — HTTP routing is one concern
4. **Merge `session` + `workspace`** — both manage persistent state
5. **Clean up duplicate types** — canonicalize definitions in home crates, remove from `core`

### Phase 3: Quality (ongoing)

1. **Resolve TODO stubs** in `cli/src/story_routes.rs`
2. **Replace placeholder test files** with real tests
3. **Run `cargo clippy`** after fixing toolchain mismatch
4. **Update documentation** to reflect new crate structure

---

## Decision Points

1. **Keep `core` or merge into `engine`?**
   - `core` has 982 lines of pure type definitions
   - If we merge `engine` + `inference` + `bnf-engine`, `core` becomes the "types" sub-module
   - **Recommendation:** Merge into `engine` as `engine::types`

2. **Keep GPU crates separate or consolidate?**
   - `rwkv7-opencl`, `rwkv7-triton`, `rwkv7-vulkan` are independent
   - They have no workspace deps, only `rwkv7-vulkan` is in workspace
   - **Recommendation:** Keep separate (good modularity for optional GPU backends)

3. **What to do with Python tmux emulator?**
   - 2,179 lines of Python, self-contained
   - Not used by Rust code, but useful for testing
   - **Recommendation:** Keep in `scripts/`, document as testing tool

---

## Next Steps

Which of these would you like me to tackle first?

- **A)** Quick wins (delete bnf-engine, fix mold, add missing workspace members)
- **B)** Crate consolidation (merge engine+inference, server+gateway, etc.)
- **C)** Type deduplication (clean up core vs home crate definitions)
- **D)** Test cleanup (replace placeholder tests, resolve TODOs)
