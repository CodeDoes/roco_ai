# Done this round

## 1. FeedEos removed from inferd

`FeedEos` is no longer an inferd primitive. The actor no longer has the
`FeedEos` message variant. `RwkvBackend::feed_eos` is now a no-op (falls
through to the trait default).

Callers that need to "reset" state between operations (e.g. validator
state between chapters) should manage their own state by saving/loading
from cache, or baking EOS text directly.

## 2. State fields renamed: `init_state` / `state_slot`

Old names → New names:

| Old | New | Meaning |
|---|---|---|
| `state_id` | `init_state` | State slot to load before processing (None = blank) |
| `save_as` | `state_slot` | State slot to save result into (None = don't cache) |
| `Bake.state_id` | `Bake.init_state` | Same |
| `Bake.name` | `Bake.state_slot` | Same |

## 3. Prose fallback parsers deleted

All 5 functions + 20+ tests deleted from story.rs:
- `prose_to_outline`, `prose_to_wiki`, `prose_to_chapter`
- `prose_to_synopsis`, `prose_to_validation`
- `prose_fallback` parameter removed from `structured_complete_with_strategy`

Phases now fail loudly on JSON parse failure instead of silently falling
back to natural language heuristics.

## Remaining (not yet done)

1. **Remove `system`/`session`/`preserve_state` from `CompletionRequest`** —
   would break ~50 callers across engine, agent, app, protocol, ui crates.
   Needs a careful pass.

2. **Remove `think_trace` from `CompletionResponse`** — tied to #1.

3. **Remove `feed_eos` from `ModelBackend` trait** — only possible after
   #1 and #2 are done and all callers updated.

4. **Update eval.rs, story_evals.rs, cases.rs** — use `init_state`/`state_slot`
   instead of `system`/`session`/`preserve_state`. These are in the engine
   crate and affect compile.

5. **Migrate agent journal to JSONL** — new sessions should use the
   JSONL format with `session-init`, `user-message`, `tool-call`, etc.

6. **Bake special token substitution** — `[EOX]` → token-0, end-sequence
   support like `[0, "asdasd"]`. Not yet implemented.

## Questions for you

1. **Bake token substitution**: Should I implement `[EOX]` → token-0
   substitution and end-sequence format parsing now, or is the current
   raw-text Bake sufficient for now?

2. **CompletionRequest cleanup**: Should I batch-fix all ~50 callers in
   one pass (removing system/session/preserve_state), or do it piecemeal?

3. **JSONL sessions**: Should I start writing new session entries in JSONL
   format in the agent journal, or wait until the full migration?
