# Tool Calling

Intent: Emit tool/function calls in a parseable format when a task requires external action.

User: Uses `structured_output` (1_infer/07) for the GBNF mechanism; the message format (2_message/01) embeds the `tool_call` grammar this builds on.
