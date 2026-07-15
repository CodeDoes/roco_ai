# State-Tuned Examples

Intent: Condition the model's behavior via saved states / few-shot examples so it adopts a desired persona or style.

Sub-goals:
- `bake_persona`: replay few-shot examples through the backend, save the resulting recurrent state
- `bake_into_session`: same but into a named session, avoiding byte-state plumbing
- Persona persistence: baked persona survives across turns without re-sending system prompt
- Eval probes: baseline → state-tuned comparison for instruction following

Reference: `crates/engine/src/backend.rs` — `bake_persona()`, `bake_into_session()`. Chat CLI `/bake` command.
