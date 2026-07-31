# Future Goals & Roadmap: RoCo AI

This document outlines the goals and next milestones for RoCo AI. These goals represent the logical evolution from our current stable vertical slice to a multi-agent, highly interactive, and natural-language friendly collaborative system.

---

## 1. Near-Term Stability & Usability Milestones

### Natural-Language First Interactions
- **Flexible Argument Parsing**: Support natural or relaxed input prompts on the CLI so a common user doesn't have to specify precise JSON schemas or options (e.g. `roco -p "A story about a lost compass"` should figure out the right mode).
- **Proactive Error fallback**: Instead of throwing raw JSON parsing errors, the framework should gracefully explain format requirements to the user and retry or clarify.

### Persistent Chat & Session Subcommands
- **Unified Chat Command**: Support structured, multi-turn chat directly via dedicated CLI flags or subcommand loops.
- **Session Lifecycle API**: Introduce `roco session create` to explicitly manage session IDs and allow repeating commands like `roco session <session_id> -p "..."` for lightweight stateless orchestration.

---

## 2. Advanced Multi-Turn Narrative Editing

- **Granular Editing Slash-Commands**:
  - `/apply <old> <new>`: Rename characters, items, or locations globally across the chapter workspaces and the world bible.
  - `/revert <num>`: Instant chapter-level backup rollback to easily recover from bad model generations or user pivots.
  - `/backups <num>`: View timeline of backups using precise standalone calendar math without bloating dependencies.
- **Context Preservation**: Avoid full transcript rewrites; optimize context window budgets to fit within the 10k RWKV-7 context limit by folding evicted history into dynamic summary blocks.

---

## 3. High-Fidelity Evaluations & Tests

- **Multi-Turn Narrative Tests**: Write integration tests simulating interactive editing sessions, character renaming, and rollbacks to verify state continuity.
- **Natural Feedback Evals**: Evaluate the model's ability to refine writing based on natural human critiques (e.g. "make the tone darker", "extend the dialogue in scene 2").
- **Offline Determinism Suite**: Ensure FIM (Fill-In-the-Middle) and grammar masks stay deterministic and do not regress over updates.

---

## 4. Long-Term Vision

- **State Pools**: Implement local thread-safe state caching to support up to 64 concurrent user session states on limited GPU VRAM.
- **State-Tuned Persona Roles**: Support specialized expert roles (e.g., worldbuilder, editor, copywriter, dialogue polisher) backed by baked state-tuning templates, maximizing quality on smaller (2.9B - 7B) models.
