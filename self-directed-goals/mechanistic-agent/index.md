# Self-Directed Goals: mechanistic-agent

Reflection of [`goals/mechanistic-agent/index.md`](../../goals/mechanistic-agent/index.md).
The mechanistic agent is a plugin that replaces the model-driven agent loop with a
**code-driven controller + router** — the model is a subroutine called only at
fixed, grammar-constrained points; classic code owns all control flow, dispatch,
and I/O.

None of this layer is implemented yet. The core agent loop (ReAct) and all its
capabilities (memory, planning, orchestration, sessions, scheduler) exist in the
`agent` layer. The mechanistic agent builds a different dispatch pattern on top.

Prerequisite order (mirrors the product layer):

1. **self_controlled_ingest** — ⬜ not started. The controller decides what the model reads; context is pulled, not pushed.
2. **intent_classification** — ⬜ not started. Classify input → route + mode; low confidence falls back to `justChatting`.
3. **task_grammars** — ⬜ not started. BNF grammars per task domain (plan, chapter, wiki, synopsis…).
4. **workspace_sandbox** — ⬜ not started. Request-scoped temp directory; model never touches the real filesystem.
5. **controller** — ⬜ not started. think → derive → dispatch → commit orchestration loop.
6. **router** — ⬜ not started. (type, domain) → handler dispatch table.
7. **modes** — ⬜ not started. Declarative route definitions: system prompt, tools, model size, state, workflow.
8. **handler_registry** — ⬜ not started. Typed (type, domain) → HandlerFn map.
9. **state_mounted_instructions** — ⬜ not started. System instructions keyed by content hash, mounted per mode.
10. **repair_loop** — ⬜ not started. Grammar-validate → structure oracle → retry → fallback.
11. **actions_gate** — ⬜ not started. Actions as the only exit to durable state; three-gate safety model.

**Self-directed priority:** Once the `message` and `workspace` layers are settled,
implement the mechanistic-agent as a concrete plugin — a `MechanisticAgent` struct
in `crates/agent/src/mechanistic.rs` that wraps or replaces the default ReAct loop.
Keep it testable via `MockBackend` with no GPU.
