# RoCo AI — Architecture Analysis & Cleanup Plan (v2)
**Date:** 2026-07-29  
**Status:** Historical — many items addressed; some remain open

> **Note**: This document describes the state as of 2026-07-29. The project has since
> undergone significant cleanup. For current architecture, see [AGENTS.md](AGENTS.md).
> 
> **User Constraints:**
> - 1 workspace, many sessions
> - Sessions can switch workspaces
> - Engine/merge only if it makes sense (trait vs implementation distinction)
> - Server/gateway merge only if performance stays same and expressivity improves

---

## Architecture Clarification

### Engine vs Inference (NOT generate vs messages)

```
┌─────────────────────────────────────────────────────────────┐
│  roco-engine (14 dependent crates)                          │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  ModelBackend trait                                 │    │
│  │  - complete(CompletionRequest) → CompletionResponse │    │
│  │  - save_state() / load_state() / mix_states()      │    │
│  │  - bake_state() / feed_eos() / interrupt()         │    │
│  ├─────────────────────────────────────────────────────┤    │
│  │  Types: CompletionRequest, CompletionResponse,     │    │
│  │         EngineError, TokenUsage, TokenTrace, etc.   │    │
│  ├─────────────────────────────────────────────────────┤    │
│  │  BNF Grammar Engine (kbnf wrapper)                 │    │
│  ├─────────────────────────────────────────────────────┤    │
│  │  MockBackend (for tests, no GPU needed)            │    │
│  ├─────────────────────────────────────────────────────┤    │
│  │  Eval framework (benchmarks, story evals)          │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                           │
                    implements
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  roco-inference (2 dependent crates)                        │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  RwkvBackend (web-rwkv + WGPU)                     │    │
│  │  - Concrete ModelBackend implementation             │    │
│  │  - Actor thread for non-Send WGPU resources        │    │
│  │  - Session pool management                          │    │
│  │  - Quantization analysis                            │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

**Why separation is good:**
- 14 crates depend on `engine` (trait + types + mock) — none need GPU
- Only `inferd` and `app` depend on `inference` (real backend)
- Merging would force GPU compilation for all 14 crates
- Build times would explode

**What `complete()` does:**
Single method that handles both "generate" and "messages" via `CompletionRequest` fields:
- `system: String` — system prompt
- `prompt: String` — user input
- `prefill: Option<String>` — assistant prefill (for state tuning)
- `thinking: bool` — enable think trace
- `grammar: Option<String>` — BNF constraint
- `session: Option<String>` — named session (recurrent state)
- `preserve_state: bool` — don't reset state between turns
- `on_token: Option<Box<dyn Fn(&str)>>` — streaming callback

### Server vs Gateway (Different Layers)

```
┌─────────────────────────────────────────────────────────────┐
│  roco-gateway (session/workspace orchestrator)              │
│  Routes: /sessions, /workspaces, /jobs                     │
│  Talks to: inferd (via HTTP)                                │
│  Manages: Session lifecycle, workspace files, job queue     │
└─────────────────────────────────────────────────────────────┘
                           │
                    HTTP calls
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  roco-inferd (inference daemon)                             │
│  Links: roco-server + roco-inference                        │
│  Routes: /complete, /bake, /vocab, /health                  │
│  Talks to: RwkvBackend (direct)                             │
└─────────────────────────────────────────────────────────────┘
```

**Why separation is good:**
- `server` = thin HTTP wrapper around `ModelBackend` (7 routes, 1,402 lines)
- `gateway` = rich orchestrator (18 routes, 1,762 lines)
- Different deployment models: server for inference-only, gateway for full API
- Gateway can survive client disconnects, server is stateless

**Merge potential:**
Could merge if gateway becomes the primary API and server routes become gateway sub-routes. But:
- Gateway already talks to inferd via HTTP (extra hop)
- Merging would make gateway depend on `roco-inference` (GPU)
- Current separation allows gateway to run without GPU

---

## Current Problems

### 1. `bnf-engine` is a Pure Wrapper (57 lines)
```rust
// crates/bnf-engine/src/lib.rs
pub use roco_engine::bnf_engine::*;
```
5 crates depend on this wrapper instead of `engine` directly. Adds indirection for no value.

### 2. `rwkv7-opencl` and `rwkv7-triton` Not in Workspace
New GPU crates added but not tracked in `Cargo.toml` workspace members.

### 3. Session Workspace Binding is Immutable
```rust
// crates/gateway/src/session/mod.rs
pub struct Session {
    pub workspace_id: String,  // Set at creation, never changes
    // ...
}
```
No `set_workspace()` method — sessions can't switch workspaces.

### 4. Duplicate Types Across Crates
- `Agent` defined 11 times (1 in agent, 10 in harness domains)
- `BakeRequest`/`BakeResponse` defined 3 times each
- `Workspace`, `Tool`, `ToolError`, `SessionStore`, etc. defined twice

### 5. `core` Crate Not in Workspace
982 lines of shared types not tracked.

### 6. Placeholder Test Files
5 identical test module files across crates.

---

## Recommended Cleanup Plan

### Phase 1: Quick Wins (1-2 hours)

| Action | Impact | Risk |
|--------|--------|------|
| Delete `crates/bnf-engine/` | Remove wrapper, update 5 deps to use `engine` | Low |
| Add `rwkv7-opencl`, `rwkv7-triton` to workspace | Track new crates | None |
| Add `core` to workspace OR merge into `engine` | Fix orphaned crate | Low |
| Add `Session::set_workspace()` method | Enable workspace switching | Low |
| Fix mold linker portability | Build on systems without mold | Low |

### Phase 2: Type Deduplication (2-4 hours)

| Action | Impact | Risk |
|--------|--------|------|
| Canonicalize `Agent` in harness (single generic struct) | Remove 10 duplicate files | Low |
| Merge duplicate `BakeRequest`/`BakeResponse` into one crate | Reduce confusion | Medium |
| Clean up `core` vs home crate type definitions | Single source of truth | Medium |

### Phase 3: Server/Gateway Merge (4-8 hours) — OPTIONAL

**Only if:** You want a single HTTP API entrypoint.

| Action | Impact | Risk |
|--------|--------|------|
| Move server routes into gateway as `/inference/*` prefix | Single binary | Medium |
| Gateway depends on `engine` (not `inference`) | No GPU in gateway | Low |
| Gateway calls inferd for inference, handles sessions/workspaces locally | Better expressivity | Medium |

**Trade-offs:**
- ✅ Single API endpoint for clients
- ✅ More expressivity (gateway can compose inference + session ops)
- ❌ Gateway becomes heavier (1,762 → ~3,000 lines)
- ❌ Extra HTTP hop eliminated (gateway → inferd becomes optional)
- ❌ Can't run gateway without GPU if it embeds server

### Phase 4: Engine/Inference Merge — NOT RECOMMENDED

**Why not:**
- Would force 14 crates to compile GPU stack
- Build times would increase dramatically
- Mock backend would need GPU dependency
- No expressivity gain (single `complete()` method already handles everything)

**Better alternative:** Keep separation, add more `ModelBackend` methods if needed.

---

## Decision Points

1. **Delete `bnf-engine` wrapper?** → YES (pure noise)

2. **Add `core` to workspace?** → Depends:
   - If `core` is intended as shared types crate: YES, add to workspace
   - If `core` is dead/redundant: DELETE and clean up duplicate types

3. **Merge server into gateway?** → DEPENDS on your goals:
   - If you want single API endpoint: YES
   - If you want to keep inference daemon separate: NO

4. **How to handle session workspace switching?** → Add mutable binding:
   ```rust
   impl Session {
       pub fn set_workspace(&mut self, workspace_id: impl Into<String>) {
           self.workspace_id = workspace_id.into();
       }
   }
   ```

---

## What I Recommend Starting With

1. **Delete `bnf-engine`** — 5 minutes, removes noise
2. **Add missing workspace members** — 2 minutes
3. **Add `Session::set_workspace()`** — 10 minutes, enables your use case
4. **Fix mold portability** — 15 minutes, improves DX

Then decide on server/gateway merge based on whether you want a single API entrypoint.
