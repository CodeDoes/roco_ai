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
4. **orchastrate** — ⬜ *self-directed (next agent cap):* build on
   `Plan::execute`. Support branch/parallel execution where steps have no
   dependency edge (run independent steps concurrently bounded by a worker
   count), and feed a step's tool result forward into dependent steps. Make
   `Plan::execute` the real orchestrator rather than a strict sequence.
5. **memory** — ✅ done. `MemoryStore` + `remember`/`recall` tools wired via
   `Agent::with_memory`. *Self-directed:* auto-inject the top-K recalled
   memories into the agent's system prefix (the `User:` note: "use a smarter
   form of prefix sampling"), so long-term context is recalled proactively.
6. **session_search** — ⬜ *self-directed:* reuse `MemoryStore`'s retrieval over
   saved session transcripts (the `session` state pool in `crates/session` plus
   a JSON transcript log). A `search_sessions` tool that ranks past runs by
   relevance to the current task.
7. **scheduled_tasks** — ⬜ *self-directed:* a `Scheduler` holding one-off and
   periodic tasks with a next-run time; a `schedule`/`run_due` tool pair. Test
   with a fake clock so it needs no real waiting.

**Wiring (my standing priority):**
- Connect `memory` + `planning` into the `agent` CLI example so a real run can
  `remember`, `recall`, and `plan` — today only `all_tools()` (unrestricted) is
  used.
- Add an **agent eval** that runs a tiny planned task against `MockBackend` and
  asserts the plan is decomposed and executed.

**Next self-directed action:** implement `orchastrate` (parallel/branch
execution on top of `Plan::execute`), then `session_search` reusing memory
retrieval.
