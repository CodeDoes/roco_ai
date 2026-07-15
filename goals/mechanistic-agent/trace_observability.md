# Trace & Observability

Intent: Per-turn structured logs that capture the full controller trace — ingested context, classified intent, selected mode, plan/task list, each handler's result, repair loop attempts, and committed actions. Enables debugging, replay, and eval without needing the model.

Sub-goals:
- Turn trace: structured record of every controller loop iteration
- Handler result capture: each handler's output logged with task metadata
- Repair loop audit: retry count, param changes, fallback triggers
- Eval replay: traces can be replayed against MockBackend for regression testing
