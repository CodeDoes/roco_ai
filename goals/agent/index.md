# Goals: agent

## Grammar-First Principle (Foundation)

**Every model call must go through a BNF grammar.** This is the non-negotiable architectural decision that underpins the entire agent system. Free-form prompting on undertrained RWKV models produces systematic contamination (`<thinking>` tags, meta-commentary) that no prompt or temperature adjustment can eliminate. Grammar-constrained decoding rejects non-conforming tokens at every sampling step — contamination cannot occur.

See `goals/infer/thinking.md` and `goals/infer/gbnf.md` for detailed learnings from live multi-stage story pipeline runs.

## Prerequisites

Prerequisite order (top to bottom):

1. **planning** — structured plan emission via GBNF grammar; no free-form JSON extraction
2. **self_prompting_chain** — model prompts itself through the structured pipeline; each step's output feeds the next query
3. **tool_execution_loop** — two modes: ReAct (open-ended, model-driven) and plan-first (predetermined, code-driven)
4. **planning** — decompose a user goal into a grammar-constrained plan with dependency tracking
5. **orchastrate** — wave-level execution with eval verification gates and dynamic subtask injection based on complexity
6. **memory** — long-term memory: retrieval, summarization, recall
7. **session_search** — searching past sessions for relevant context
8. **scheduled_tasks** — cron-like recurring or deferred tasks


## Status & Self-Directed Actions

the remaining capabilities plus end-to-end wiring.

Prerequisite order (mirrors the product layer):

1. **agent** ✅ done (core ReAct loop in `crates/agent/src/agent.rs`).
2. **tool_execution_loop** ✅ done.
3. **planning** ✅ done. `Planner` + `Plan` with defensive JSON extraction
   (falls back to a single step), `topological_order`, (de)serialization, and
   sequential `Plan::execute`. *Self-directed improvement:* optionally
   grammar-constrain the planner's JSON output via `roco_grammar::schema_to_gbnf`
   so the 2.9B model emits cleaner plans; keep the fallback as the safety net.
4. **orchastrate** ✅ done. `Plan::execute` now groups steps into
   dependency **waves** and runs each wave concurrently via `join_all`, threading
   results forward into later waves; outcomes are returned in stable topological
   order. Independent steps branch in parallel, dependent steps wait. A
   `wave_levels` helper drives this and is unit-tested.
5. **memory** ✅ done. `MemoryStore` + `remember`/`recall` tools wired via
   `Agent::with_memory`. *Self-directed:* auto-inject the top-K recalled
   memories into the agent's system prefix (the `User:` note: "use a smarter
   form of prefix sampling"), so long-term context is recalled proactively.
6. **session_search** ✅ done. `SessionStore`
   (`crates/agent/src/sessions.rs`) records agent runs as `SessionTranscript`s
   (with `from_trace`) and exposes a `search_sessions` tool that ranks past
   sessions by content using the shared `score_text` ranker from `memory`.
   Wired via `Agent::with_sessions`.
7. **scheduled_tasks** ✅ done. `Scheduler`
   (`crates/agent/src/scheduler.rs`) with one-off + periodic tasks, an injectable
   fake clock, `due()` / `run_due(backend)` (one-off removed, periodic
   rescheduled), JSON persistence, and a `schedule` tool wired via
   `Agent::with_scheduler`. `run_due` is host-driven (not a model tool).

**Wiring (my standing priority):**
- ✅ Connect `memory` + `planning` + `session_search` + `scheduler` into the `agent`
  CLI example so a real run can `remember`, `recall`, `plan`, `search_sessions`,
  and `schedule` — previously only `all_tools()` (unrestricted) was used. The
  example now builds a `Workspace`-sandboxed agent with `MemoryStore`,
  `SessionStore`, and `Scheduler`, records its run, and runs due tasks.
- ✅ Add an **agent eval** that runs a tiny planned task against `MockBackend` and
  asserts the plan is decomposed and executed (`agent_runs_with_combined_…`).

**Next self-directed action:** return to the `workspace` layer — add a sandbox-
escape eval case that asserts `Workspace::resolve` rejects path traversal — and
then the `message` layer's remaining items (`state_tune_examples`,
`system_instruction_following`, `user_message_response`).
