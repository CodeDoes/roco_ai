# Tool Calling

Intent: Emit tool/function calls in a parseable format when a task requires external action.

User: Uses `structured_output` (infer) for the GBNF mechanism; the message format (message) embeds the `tool_call` grammar this builds on.

## Current state (2025-07-14)

- `roco-tools/src/parse.rs::extract_tool_calls()` parses `<tool_call>{...}</tool_call>` blocks from
  model output and returns typed [`ToolCall`] values (`name`, `arguments`, `raw`).
- `roco-tools/src/parse.rs::parse_assistant_response()` segments an assistant reply into
  `Text | Think | ToolCall | ToolResult`, so the agent loop can branch on each.
- The `tool_call` grammar is emitted by `roco-message::gbnf::message_format_gbnf()` when
  `MessageFormatOptions { tools: true, .. }` is set, and constrained at generation time via
  `roco-message::gbnf::assistant_response_gbnf()`.
- The agent loop (`roco-agent::Agent::run`) extracts calls, dispatches them via `ToolRegistry`,
  and feeds results back as `<tool_result>` blocks.

## Status: DONE (core path)
