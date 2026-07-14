# Self-Directed Goals: message

Reflection of [`goals/message/index.md`](../../goals/message/index.md). The
core chat protocol is done; my self-directed work is the remaining items plus
making the chat CLI actually exercise the state-mixing feature.

Prerequisite order (mirrors the product layer):

1. **message_format_gbnf** — ✅ done.
2. **system_instruction_following** — 🟡 *self-directed:* add an eval case that
   probes baseline adherence without state-tuning, so we know the model's
   starting point (the `User:` note asks exactly this).
3. **user_message_response** — 🟡 *self-directed:* add a coherence/format eval
   case for a plain user turn.
4. **state_tune_examples** — ⬜ *self-directed:* use `bake_persona` to persist a
   few-shot state and verify it changes behavior; expose via the chat CLI.
5. **tool_catelogue** — ✅ done.
6. **tool_calling** — ✅ done.
7. **tool_result_handling** — ✅ done.
8. **gradual_tool_disclosure** — ⬜ *self-directed:* instead of dumping all six
   tool schemas into every prompt, disclose only the tools relevant to the
   current task (match by keyword against the task + recent memory). Wire into
   `Agent`'s tool schema rendering.
9. **error_recovery** — ✅ done (`complete_with_retry`).
10. **chat_cli** — ✅ done. `crates/cli/examples/chat.rs` now drives
    multi-turn conversation via `CompletionRequest::session` (the Phase-1 state
    pool carries the context, not a rebuilt prompt) and adds `/save`, `/load`,
    `/system`. System prompt is folded into the recurrent state on the first
    turn of a session, then the state carries it.

**Next self-directed action:** add the `system_instruction_following` and
`user_message_response` eval cases, then `gradual_tool_disclosure`.
