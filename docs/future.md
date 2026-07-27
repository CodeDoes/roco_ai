# Future Work — UX, Modularity, Determinism, Interpretability

This document catalogs concrete improvements beyond the current state, organized by the four pillars. Each item includes rationale and a suggested approach.

---

## Modularity — Crate Architecture & Separation of Concerns

### 6. Consolidate 16 crates → 7 core crates (revisit RFC 0001)
**Status:** In Progress (Consolidated `message`/`chat-common` → `roco-protocol`, `validation`/`tools` → `roco-agent`, `grammar` → `roco-engine`, `workspace` re-exported under `roco-session`)

**Rationale:** The workspace has 16 crates (down from 19). Cross-crate changes still require touching 3–5 crates. Build times suffer from the compilation unit boundary overhead.

**Proposed consolidation (builds on `AGENTS.md`):**

| Current Crates | Proposed Crate | Status | Rationale |
|---|---|---|---|
| `message`, `chat-common`, `protocol` | `roco-protocol` | Done | All define wire types in one crate |
| `agent`, `validation`, `tools` | `roco-agent` | Done | Agent loop, verifiers, and tool registry unified |
| `engine`, `inference`, `grammar`, `bnf-engine` | `roco-engine` | Partial | Grammar module merged into `roco-engine` |
| `session`, `workspace` | `roco-session` | Done | Workspace persistence re-exported under `roco-session` |
| `gateway`, `server`, `infer-client` | `roco-net` | Feature-gated | Transport layer g-feature in `roco-cli` |
| `cli`, `app` | `roco-cli` | Done | Primary binary |
| `ui` | `roco-ui` | Done | Desktop GUI is self-contained |
| `harness` | `roco-harness` | Done | Evaluation harness standalone |

---

## Summary of Priorities

| Priority | Area | Item | Effort | Impact |
|----------|------|------|--------|--------|
| P1 | Modularity | 6. Crate consolidation | 3-5 days | High — halves build time, simplifies navigation |
