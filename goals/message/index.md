# Goals: message

## Grammar-First Principle (Foundation)

**Every model call must go through a BNF grammar.** The message layer enforces this through `message_format_gbnf` and `assistant_response_gbnf`. Free-form prompting on undertrained RWKV models produces systematic contamination (`<thinking>` tags, meta-commentary) that no prompt or temperature adjustment can eliminate. Grammar-constrained decoding rejects non-conforming tokens at every sampling step.

See `goals/infer/thinking.md` and `goals/message/error_recovery.md` for learnings.

## Prerequisites

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


## Status & Self-Directed Actions

making the chat CLI actually exercise the state-mixing feature.

Prerequisite order (mirrors the product layer):

1. **message_format_gbnf** ✅ done.
2. **system_instruction_following** ✅ done (self-directed). `message_eval_cases()`
   in `crates/engine/src/cases.rs` adds `instruct_baseline_persona`, an eval
   case that probes the *un-tuned* model's adherence to a system persona/format
   constraint. Wired into the `eval_suite` example (run against the RWKV
   backend, since `MockBackend` is non-semantic) and asserted present by a unit
   test.
3. **user_message_response** ✅ done (self-directed). `message_eval_cases()`
   adds `user_turn_coherence`, a coherence/format probe for a plain user turn,
   wired and unit-tested the same way.
4. **state_tune_examples** ✅ done (self-directed). `bake_into_session`
   (`crates/engine/src/backend.rs`) bakes a few-shot persona into a *named
   session* via `preserve_state` (so `RwkvBackend`'s session pool carries the
   persona, not a rebuilt prompt — unlike the byte-based `bake_persona`, which
   only works for backends implementing `save_state`/`load_state`). Exposed in
   the chat CLI as `/bake <file>` (tagged `user:`/`assistant:` pairs), which
   folds the persona into the current session state and is unit-tested for
   plumbing via `MockBackend`.

**Next self-directed action:** the `message` layer's self-directed items are
all done; return to the product `goals/message` remaining sub-goals or move to
`agent_chat` (folder-bound persistent agent sessions).
5. **tool_catelogue** ✅ done.
6. **tool_calling** ✅ done.
7. **tool_result_handling** ✅ done.
8. **gradual_tool_disclosure** ⬜ *self-directed:* instead of dumping all six
   tool schemas into every prompt, disclose only the tools relevant to the
   current task (match by keyword against the task + recent memory). Wire into
   `Agent`'s tool schema rendering.
9. **error_recovery** ✅ done (`complete_with_retry`).
10. **chat_cli** ✅ done. `crates/cli/examples/chat.rs` now drives
    multi-turn conversation via `CompletionRequest::session` (the Phase-1 state
    pool carries the context, not a rebuilt prompt) and adds `/save`, `/load`,
    `/system`. System prompt is folded into the recurrent state on the first
    turn of a session, then the state carries it.
8. **gradual_tool_disclosure** ✅ done. `select_relevant`
   (`crates/agent/src/tool_selector.rs`) discloses only task-relevant tools
   (keyword-overlap score over name+description, reusing `memory`'s ranker),
   with a safety net that returns all tools when none score above zero. Wired
   into `Agent` via `AgentConfig::gradual_tool_disclosure`.

**Next self-directed action:** implement `state_tune_examples` — use
`bake_persona` to persist a few-shot state and verify it changes behavior,
expose via the chat CLI (e.g. a `/bake` command), then revisit the `message`
layer's remaining product sub-goals.

