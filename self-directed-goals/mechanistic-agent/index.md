# Self-Directed Goals: mechanistic-agent

Reflection of [`goals/mechanistic-agent/index.md`](../../goals/mechanistic-agent/index.md).
The mechanistic agent is a plugin that replaces the model-driven agent loop with a
**code-driven controller + router** — the model is a subroutine called only at
fixed, grammar-constrained points; classic code owns all control flow, dispatch,
and I/O.

The core `MechanisticAgent` struct is implemented in
`crates/agent/src/mechanistic.rs` with think → derive → dispatch → commit
loop, typed task/plan types, a (type, domain) → HandlerFn router, and
6 unit tests against MockBackend. The core agent loop (ReAct) and all its
capabilities (memory, planning, orchestration, sessions, scheduler) exist
in the `agent` layer. The mechanistic agent builds a different dispatch
pattern on top.

Prerequisite order (mirrors the product layer):

1. **self_controlled_ingest** — 🟡 partial. `MechanisticAgent::think()` calls the model; context is the user message. No pull protocol yet.
2. **intent_classification** — ⬜ not started. No route classification yet; the agent uses a fixed plan grammar.
3. **task_grammars** — ✅ done. `PLAN_GRAMMAR` BNF constrains model output to a valid Plan JSON with typed tasks.
4. **workspace_sandbox** — ✅ done. `run()` creates a `Workspace::temp()` sandbox; handlers write through `ws.resolve()`; `commit()` snapshots all files into `MechanisticOutcome::workspace_files`.
5. **controller** — ✅ done. think → repair_derive → dispatch → commit loop in `MechanisticAgent::run()`.
6. **router** — ✅ done. (type, domain) → HandlerFn dispatch table; unknown pairs fail loud.
7. **modes** — ⬜ not started. No mode system yet; handlers registered directly.
8. **handler_registry** — ✅ done. Typed HashMap-based registry with `register()` API.
9. **state_mounted_instructions** — ⬜ not started. System prompt passed directly, not state-mounted.
10. **repair_loop** — ✅ done. `RepairConfig` + `repair_derive()` wraps `derive()` with retry, temperature decay, token truncation, and bounded retries. 3 unit tests cover retry exhaustion, zero-retry mode, and param tightening.
11. **actions_gate** — ⬜ not started. Handler results collected but not gated by action registry.

**Self-directed priority:** Core + repair loop + workspace sandbox done with 9
unit tests against MockBackend. Next: intent classification for mode routing,
then state-mounted instructions.
