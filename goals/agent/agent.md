# Agent

Intent: The autonomous loop that ties inference, tools, and memory together to pursue user goals.

## Current state (2025-07-14)

- `roco-agent::Agent` implements the ReAct loop: build prompt → generate (constrained by
  `assistant_response_gbnf`) → parse segments → if tool calls, execute via `ToolRegistry` and
  feed `<tool_result>` back → loop; if final text, stop.
- `AgentConfig` exposes `system_prompt`, `max_steps`, `budget_tokens`, `temperature`,
  `enable_think`, `enable_tools`, `verbose`.
- `AgentTrace` records every step (`AgentStep`) with tool calls/results and total token usage.
- `Agent::run_subtask()` runs a single `Subtask` as one model call (for planning decomposition).
- 4 unit tests cover: final-text (no tools), tool-call path, step-limit termination, config sanity.

## Status: DONE (core loop)
