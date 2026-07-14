# Tool Result Handling

Intent: Parse tool results (`<tool_result>`) returned by the environment and feed them back into the model context to continue the turn.

## Current state (2025-07-14)

- The agent loop (`roco-agent::Agent::run`) executes each `ToolCall`, serializes the `Value`
  result to a string, and appends `<tool_call>{raw}</tool_call><tool_result>{result}</tool_result>`
  to the conversation `history` for the next model turn.
- `roco-tools::parse::AssistantSegment::ToolResult` is produced when the model emits a
  `<tool_result>` block itself (defensive parsing; normally the agent injects results).
- `roco-message::error::complete_with_retry()` wraps tool-augmented generation with grammar
  fallback + retry so a transient failure doesn't abort the tool loop.

## Status: DONE
