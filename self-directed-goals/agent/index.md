# Self-Directed Goals: agent

Reflection of [`goals/agent/index.md`](../../goals/agent/index.md). Core loop,
tool execution, **memory**, and **planning** are done. My self-directed work is
the remaining capabilities plus end-to-end wiring.

Prerequisite order (mirrors the product layer):

1. **agent** — ✅ done (core ReAct loop in `crates/agent/src/agent.rs`).
2. **tool_execution_loop** — ✅ done.
3. **planning** — ✅ done. `Planner` + `Plan` with defensive JSON extraction
   (falls back to a single step), `topological_order`, (de)serialization, and
   sequential `Plan::execute`. *Self-directed improvement:* optionally
   grammar-constrain the planner's JSON output via `roco_grammar::schema_to_gbnf`
   so the 2.9B model emits cleaner plans; keep the fallback as the safety net.
4. **orchastrate** — ✅ done. `Plan::execute` now groups steps into
   dependency **waves** and runs each wave concurrently via `join_all`, threading
   results forward into later waves; outcomes are returned in stable topological
   order. Independent steps branch in parallel, dependent steps wait. A
   `wave_levels` helper drives this and is unit-tested.
5. **memory** — ✅ done. `MemoryStore` + `remember`/`recall` tools wired via
   `Agent::with_memory`. *Self-directed:* auto-inject the top-K recalled
   memories into the agent's system prefix (the `User:` note: "use a smarter
   form of prefix sampling"), so long-term context is recalled proactively.
6. **session_search** — ✅ done. `SessionStore`
   (`crates/agent/src/sessions.rs`) records agent runs as `SessionTranscript`s
   (with `from_trace`) and exposes a `search_sessions` tool that ranks past
   sessions by content using the shared `score_text` ranker from `memory`.
   Wired via `Agent::with_sessions`.
7. **scheduled_tasks** — ⬜ *self-directed:* a `Scheduler` holding one-off and
   periodic tasks with a next-run time; a `schedule`/`run_due` tool pair. Test
   with a fake clock so it needs no real waiting.

**Wiring (my standing priority):**
- Connect `memory` + `planning` into the `agent` CLI example so a real run can
  `remember`, `recall`, and `plan` — today only `all_tools()` (unrestricted) is
  used.
- Add an **agent eval** that runs a tiny planned task against `MockBackend` and
  asserts the plan is decomposed and executed.

**Next self-directed action:** implement `scheduled_tasks` (a fake-clock
`Scheduler` + `schedule`/`run_due` tools), then wire memory + planning +
session-search into the `agent` CLI example so a real run can `remember`,
`recall`, `plan`, and `search_sessions`.
