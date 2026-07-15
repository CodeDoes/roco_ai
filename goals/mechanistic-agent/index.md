# Goals: mechanistic-agent

A plugin layer on top of the core agent loop. The mechanistic agent replaces the model-driven agent loop with a **code-driven controller + router** — the model is a subroutine called only at fixed, grammar-constrained points; classic code owns all control flow, dispatch, and I/O.

Prerequisite order (top to bottom):

1. **self_controlled_ingest** — the controller decides what the model reads; context is pulled, not pushed
2. **intent_classification** — classify user input → route + mode selection; low confidence falls back to `justChatting`
3. **task_grammars** — BNF grammars per task domain; model output is structurally trusted
4. **workspace_sandbox** — request-scoped temp directory; model never touches the real filesystem
5. **controller** — think → derive → dispatch → commit orchestration loop
6. **router** — (type, domain) → handler dispatch table; unknown pairs fail loud
7. **mode_file_format** — the `.mode` DSL: route, system, model, tools/tasks, state, loop, exit_codes, examples
8. **modes** — declarative route definitions (system prompt, tools, model size, state, workflow loop); router dispatches here
9. **fallback_chains** — modes declare fallback chains; low confidence or retry exhaustion routes to the next mode; terminal fallback is always `justChatting`
10. **handler_registry** — typed (type, domain) → HandlerFn map; modes register handlers; unknown pairs fail loud
11. **state_mounted_instructions** — system instructions keyed by content hash, mounted per mode, not in-prompt
12. **repair_loop** — grammar validate, structure oracle, retry with tightened params, fallback
13. **actions_gate** — actions as the only exit to durable state; three-gate safety model
14. **trace_observability** — per-turn structured logs capture the full controller trace for debugging, replay, and eval
