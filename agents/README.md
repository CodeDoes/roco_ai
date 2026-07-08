# Agents

Per-agent configuration lives under `agents/<role>/`, where `<role>` is any agent
in the orchestration hierarchy (the `*` in `agents/*/`). Each agent directory
contains:

- `tools/`        — tool / function definitions the agent may call
- `instructions/` — system prompt / task instructions (schema-first, §2.2)
- `skills/`       — composable capability modules
- `examples/`     — few-shot examples for one-shot prompting (§2.2E)

Currently instantiated roles (matching `src/agent.rs`):

- `orchestrator/` — decomposes tasks, dispatches to workers, aggregates results
- `worker/`      — 3B sub-agent executing a single atomic subtask
- `verifier/`    — judge / checklist verification gate (§3, §5.2)

Add a new agent by creating `agents/<new_role>/` with the four sub-directories
above.
