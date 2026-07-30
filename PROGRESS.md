# Refactoring: Stripping Format Logic from inferd

## Change Made

**inferd now only receives raw text.** The `RwkvActor` no longer adds
`System:/User:/Assistant:` wrappers, `<think>` suppression, or implicit
session-based state management. All formatting is the caller's (gateway)
responsibility.

### Files Changed

| File | Change |
|---|---|
| `crates/engine-gpu/src/actor.rs` | `CompleteReq`: replaced `system`/`preserve_state`/`session`/`thinking` with `state_id`/`save_as`. Added `Bake` message. `handle_complete`: removed all formatting — uses `prompt` as raw text. `FeedEos`: restored old behavior — loads session state, feeds token 0, saves back to pool. |
| `crates/engine-gpu/src/backend.rs` | `RwkvBackend.complete()`: maps old `session` → `state_id` + `save_as` (always both, matching old implicit persistence). `tune_state`: uses `Bake` message with formatted `System:\n\nUser:\n\nAssistant:` text. |
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

### Bugs Fixed

1. **Backward compat mapping (session → state_id/save_as)** — Old code always
   saved state when `session` was set (implicit persistence). The initial map
   only saved when `preserve_state=true`, meaning chapters 2+ loaded the baked
   state instead of continuing from chapter 1's output. Fixed: both `state_id`
   and `save_as` are set whenever `session` is set.

2. **feed_eos (state reset between chapters)** — New code just reset the
   in-memory state to blank and didn't touch the pool. Old code loaded the
   session state, fed token 0 (EOS) through the model, and saved back to pool.
   This prevented repetition patterns from bleeding between chapters. Fixed:
   restored old behavior.

3. **Mock backend keyword matching** — MockBackend was matching on `req.system`
   (which is now empty) instead of `req.prompt`. Fixed: match on prompt
   keywords with unique phase identifiers (outliner, worldbuilding, etc.).

### Known Issues (model-level, pre-existing)

1. **No progress indicator during model build** — 4+ minutes of silence
2. **Model enters repetition loops** at temperature >= 0.7 (2.9B RNN limitation)
3. **JSON output vs prose** — outline/wiki/chapter phases need prose fallback
   parsers because the 2.9B model outputs prose, not JSON
4. **Control characters in output** — model sometimes emits null bytes

### Next Steps

- [x] Remove formatting logic from actor
- [x] Add `Bake` message to actor
- [x] Update RwkvBackend to map old→new fields
- [x] Update mock backend to use prompt text
- [x] Update CLI to format prompts with system
- [x] Fix backward compat mapping (session → always save)
- [x] Fix feed_eos to properly EOS-process pool state
- [ ] Run E2E test with real model
- [ ] Verify chapters are unique and follow outline
- [ ] Add regression tests for the bugs found
