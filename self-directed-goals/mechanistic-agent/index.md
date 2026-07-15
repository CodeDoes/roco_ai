# Self-Directed Goals: mechanistic-agent

Reflection of [`goals/mechanistic-agent/index.md`](../../goals/mechanistic-agent/index.md).
The mechanistic agent is a plugin that replaces the model-driven agent loop with a
**code-driven controller + router** — the model is a subroutine called only at
fixed, grammar-constrained points; classic code owns all control flow, dispatch,
and I/O.

The core `MechanisticAgent` struct is implemented in
`crates/agent/src/mecha_agent.rs` with think → derive → dispatch → commit
loop, typed task/plan types, a (type, domain) → HandlerFn router, and
unit tests against MockBackend. The core agent loop (ReAct) and all its
capabilities (memory, planning, orchestration, sessions, scheduler) exist
in the `agent` layer. The mechanistic agent builds a different dispatch
pattern on top.

## Lessons Learned from Live Generation
### Grammar-First Over Workarounds
Live story pipeline runs on undertrained RWKV models proved:
- **GBNF grammars prevent contamination at source** — no post-processing needed
- **Workarounds are interim signals** — pre-fill think blocks + strip_think_blocks mark where grammars still needed
- **Architecture decisions proven correct**: code owns control flow, LLM fires only at grammar-bounded points, pull-based context injection over push

### What Didn't Work (Documented for Future Reference)
- System prompts preventing `thinking>` leakage → zero effect regardless of strength
- Temperature decay stopping contamination → model leaks at all temperatures
- Character-by-character think block stripping → closing tags never detected, open-ended blocks dominate
- Fallback returning raw text → defeats entire purpose of cleaning
- Over-engineered parsers → simple string replace beats regex/state-machines for this use case

### Key Patterns That Worked
- Pre-fill `thinking>...plan...</thi nk>` before prompt tricks model into clean output
- Arc-owned context sources cleanly satisfy `'static` bounds
- Jaccard word overlap relevance scoring sufficient for initial context management
- Persistent timestamped workspaces prevent collision across repeated runs
- Self-correction detection (validation failures → retry with corrections)

## Prerequisite Order (with Current Status & Grammars Needed)

1. **self_controlled_ingest** — ✅ done. ContextManager pulls snippets by relevance-sorted sources, applies token budget per inference call. Wired into MechanisticAgent with `with_session_store()`, `with_memory_store()`, `with_context_budget()` builders.
2. **intent_classification** — ✅ done. `classify()` uses `INTENT_GRAMMAR` → structured `Intent`. Confidence routing works. 3 tests.
3. **task_grammars** — 🔴 **critical gap.** Plan-level `PLAN_GRAMMAR` exists but per-domain stage grammars are missing. Every story handler (outline, wiki, chapter, validation, synopsis) currently uses free-form prompting with pre-fill workaround. This is the #1 architectural debt.
4. **workspace_sandbox** — ✅ done. Handlers write through `ws.resolve()`; `commit()` snapshots to `MechanisticOutcome::workspace_files`. Persistent `.roco/workspaces/story_<prompt>_<ts>/` confirmed working.
5. **controller** — ✅ done. Deterministic pipeline: outline → wiki → chapters ×3 (validate+retry) → synopsis → publish.
6. **router** — ✅ done. `(type, domain)` → HandlerFn dispatch; route validation via `validate_route_tasks()`.
7. **modes** — 🟡 partial. Routes via `add_route()` + intent picking. `.mode` DSL explicitly scrapped — code-only route definition preferred.
8. **handler_registry** — ✅ done. Typed HashMap-based registry with `register()` API. 6 built-in tools registered.
9. **state_mounted_instructions** — ⬜ not started. System prompt inline, not hash-keyed. Would improve prompt efficiency for long multi-step runs.
10. **repair_loop** — ✅ done. `RepairConfig` + `repair_derive()` with retry, temp decay, token truncation. Story example has additional validate→retry chain for chapter quality.
11. **actions_gate** — ⬜ not started. Handler results collected but no schema gate before filesystem writes.

**Self-directed priority shift:** Shifted from "mode file parser (.mode DSL)" → **per-handler BNF grammars**. The .mode DSL was scrapped anyway (user rejected declarative mode files). Immediate next steps:
1. Wire `BnfConstraint` into outline handler → generate `outline.bnf` from JSON schema
2. Wire `BnfConstraint` into wiki handler → `wiki.bnf` for character bios + setting lore
3. Wire `BnfConstraint` into chapter handlers → `chapter_prose.bnf` template
4. Wire `BnfConstraint` into validation handler → `validation_report.bnf`
5. Wire `BnfConstraint` into synopsis handler → `synopsis.bnf`
6. Remove `clean_complete` pre-fill workaround once all stages are grammar-bound
7. Remove `strip_think_blocks` entirely once no free-form calls remain
