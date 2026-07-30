# Refactoring: Stripping Format Logic from inferd

## Change Made

**inferd now only receives raw text.** The `RwkvActor` no longer adds
`System:/User:/Assistant:` wrappers, `<think>` suppression, or implicit
session-based state management. All formatting is the caller's (gateway)
responsibility.

### Files Changed

| File | Change |
|---|---|
| `crates/engine-gpu/src/actor.rs` | `CompleteReq`: replaced `system`/`preserve_state`/`session`/`thinking` with `state_id`/`save_as`. Added `Bake` message. `handle_complete`: removed all formatting — uses `prompt` as raw text. |
| `crates/engine-gpu/src/backend.rs` | `RwkvBackend`: maps old `session`/`preserve_state` to new `state_id`/`save_as`. `tune_state`: uses `Bake` message with formatted `System:\n\nUser:\n\nAssistant:` text. |
| `crates/engine/src/backend.rs` | MockBackend: routes on `prompt` keywords instead of `system` keywords. |
| `crates/cli/src/cmd/story.rs` | `structured_complete_with_strategy`: formats `system`+`prompt` into raw text before sending. |

### What Changed Semantically

**Before:** actor did:
```
if use_system { "System: {sys}\n\nUser: {prompt}\n\nAssistant:" }
else { "User: {prompt}\n\nAssistant:" }
```
And `bake_state` used `CompleteReq` with `preserve_state=true, max_tokens=0`,
but with `system=""` — so examples were formatted as `"User: ...\n\nAssistant:..."`,
DIFFERENT from the real generation format.

**After:** actor just uses `prompt` as-is. Gateway formats:
```
"System: {sys}\n\nUser: {prompt}\n\nAssistant:"
```
Bake uses same format via `Bake` message. Same format = same state conditioning.

### Bake State Flow (Backward Compat)

The old `bake_state()` → `tune_state()` path still works. It now sends
`ActorMessage::Bake` with the formatted text `"System: ...\n\nUser: ...\n\nAssistant:{response}"`.
Examples are accumulated into a single named state via repeated `Bake` calls.

### Next Steps

- [x] Remove formatting logic from actor
- [x] Add `Bake` message to actor
- [x] Update RwkvBackend to map old→new fields
- [x] Update mock backend to use prompt text
- [x] Update CLI to format prompts with system
- [ ] Run E2E test with real model
- [ ] Verify chapters follow outline (or diagnose model limitation)
