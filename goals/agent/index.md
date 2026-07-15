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
