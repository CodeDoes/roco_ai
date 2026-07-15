# Goals: agent

Prerequisite order (top to bottom):

1. **planning** — structured plan emission via GBNF grammar; no free-form JSON extraction
2. **self_prompting_chain** — model prompts itself through the structured pipeline; each step's output feeds the next query
3. **tool_execution_loop** — two modes: ReAct (open-ended, model-driven) and plan-first (predetermined, code-driven)
4. **planning** — decompose a user goal into a grammar-constrained plan with dependency tracking
5. **orchastrate** — wave-level execution with eval verification gates and dynamic subtask injection based on complexity
6. **memory** — long-term memory: retrieval, summarization, recall
7. **session_search** — searching past sessions for relevant context
8. **scheduled_tasks** — cron-like recurring or deferred tasks
