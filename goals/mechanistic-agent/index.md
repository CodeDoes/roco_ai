# Goals: mechanistic-agent

A plugin layer on top of the core agent loop. The mechanistic agent replaces the model-driven agent loop with a **code-driven controller + router** — the model is a subroutine called only at fixed, grammar-constrained points; classic code owns all control flow, dispatch, and I/O.

Prerequisite order (top to bottom):

1. **self_controlled_ingest** — the controller decides what the model reads; context is pulled, not pushed
2. **task_grammars** — BNF grammars per task domain; model output is structurally trusted
3. **workspace_sandbox** — request-scoped temp directory; model never touches the real filesystem
4. **controller** — think → derive → dispatch → commit orchestration loop
5. **router** — (type, domain) → handler dispatch table; unknown pairs fail loud
6. **modes** — declarative route definitions (system prompt, tools, model size, state, workflow loop); router dispatches here
7. **repair_loop** — grammar validate, structure oracle, retry with tightened params, fallback
8. **actions_gate** — actions as the only exit to durable state; three-gate safety model
