# Goals: message

Prerequisite order (top to bottom):

1. **message_format_gbnf** — the GBNF grammar for the agent↔user message format
2. **system_instruction_following** — system prompt / instruction adherence
3. **user_message_response** — how the model responds to user messages
4. **state_tune_examples** — few-shot examples to steer model behavior
5. **tool_catelogue** — the registry of available tool schemas
6. **tool_calling** — tool call format and dispatch
7. **tool_result_handling** — injecting tool results back into context
8. **gradual_tool_disclosure** — showing only relevant tools, not the full list
9. **error_recovery** — handling malformed tool calls, retries, fallbacks
10. **chat_cli** — interactive terminal REPL for chatting with the model
