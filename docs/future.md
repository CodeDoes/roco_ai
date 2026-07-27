# Future Work — UX, Modularity, Determinism, Interpretability

This document catalogs concrete improvements beyond the current state, organized by the four pillars. Each item includes rationale and a suggested approach.

---

## UX — User Experience

---

---

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

## Interpretability — Understanding What the Model Is Doing

---

### 15. Token probability heatmap in the TUI/GUI
**Rationale:** The desktop GUI (`crates/ui`) has no way to show uncertainty. Users see the final text but not the model's confidence.

**Approach:**
- Add a "confidence" color overlay to the markdown editor: green (p > 0.9), yellow (0.5-0.9), red (< 0.5)
- Show per-token probability on hover
- Requires the trace data from item 14

---

---

---

---

---

## Testing & CI

---

---


---

## Story Pipeline Fault Tolerance & Pipeline Safety



---

## Build & Developer Experience


---

---

## Summary of Priorities

| Priority | Area | Item | Effort | Impact |
|----------|------|------|--------|--------|
| P1 | Modularity | 6. Crate consolidation | 3-5 days | High — halves build time, simplifies navigation |
