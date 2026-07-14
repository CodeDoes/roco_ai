# Goals: agent

Prerequisite order (top to bottom):

1. **agent** — the core agent loop (plan → act → observe → reflect)
2. **tool_execution_loop** — the tool dispatch → result → retry cycle
3. **planning** — the agent's internal planning and task decomposition
4. **orchastrate** — orchestration: coordinating multiple sub-tasks / tool chains
5. **memory** — long-term memory: retrieval, summarization, recall
6. **session_search** — searching past sessions for relevant context
7. **scheduled_tasks** — cron-like recurring or deferred tasks
