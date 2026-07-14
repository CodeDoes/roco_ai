# Tool Execution Loop

Intent: The core observe→think→act (ReAct) cycle: decide, call a tool, ingest the result, and repeat until the goal is met.

## Current state (2025-07-14)

Implemented inside `roco-agent::Agent::run`:

1. **observe** — render prompt from system + task + accumulated `history`.
2. **think** — model may emit `<think>...</think>` (parsed, optionally logged).
3. **act** — model emits `<tool_call>{name, arguments}`; `Agent::execute_tool` looks up the
   tool in `ToolRegistry`, calls it, and serializes the `Value` result.
4. **ingest** — result is wrapped in `<tool_result>` and appended to `history`.
5. **repeat** — loop until no tool calls remain (final answer) or `max_steps`/`budget_tokens` hit.

Transient failures use `roco-message::error::complete_with_retry`. Tool-not-found and tool errors
return a JSON `{"error": ...}` so the model can recover on the next turn.

## Status: DONE
