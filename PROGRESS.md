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

## 4. Temporary deprecation stubs added (current commit)

Added backward-compatibility stubs to restore compilation while the
full cleanup continues:
- `bake_state` compatibility method on `ModelBackend` trait
- Deprecated fields (`system`, `session`, `preserve_state`, `thinking`)
  back on `CompletionRequest` (marked for future removal)
- `session` field on `OpenAiCompletionRequest` in protocol types
- Updated struct initializers across engine crate

These stubs preserve the architecture intent (inferd = pure token engine,
frontend = formatting/orchestration) while avoiding cascading breakage
from bulk refactoring. They will be removed in a dedicated cleanup pass
once all callers have been migrated.

## Current status

✅ All crates compile cleanly (`cargo check --workspace`)
✅ `roco-inferd` builds successfully
✅ `roco-server` compiles
✅ `roco-engine-gpu` and `roco-engine` compile
⚠️ Only benign unused variable warnings remain in agent/cli crates

## Next phase: systematic migration plan

### Approach: Conservative one-file-at-a-time with verification

Instead of bulk replacements that introduced errors earlier, we'll:
1. Verify compilation baseline (✅ done)
2. Fix one validation module at a time
3. Run `cargo check --workspace` after each change
4. Only proceed when build passes
5. Document each migration in PROGRESS.md

### Migration pattern for each validation module

**Before:**
```rust
CompletionRequest::builder()
    .prompt(user_input)
    .system(system_prompt)
    .session("session-id")
    .preserve_state(true)
    .build()
```

**After:**
```rust
CompletionRequest::builder()
    .prompt(format!("System: {}\n\nUser: \n\nAssistant:", system_prompt))
    .init_state(Some("session-id".to_string()))
    .state_slot(Some("session-id".to_string()))
    .preserve_state(false) // Will be removed later
    .build()
```

### Files to migrate (in order):

| File | Priority | Status |
|------|----------|--------|
| intent.rs | High | ⏳ Pending |
| planner.rs | High | ⏳ Pending |
| summarizer.rs | Medium | ⏳ Pending |
| wiki.rs | Medium | ⏳ Pending |
| brainstorm.rs | Medium | ⏳ Pending |
| inference.rs | Medium | ⏳ Pending |
| agent.rs | Low | ⏳ Pending |

### After validating: Continue with LSP, CLI, server, and protocol cleanup.
`preserve_state`, `thinking` fields from all types.

### Phase 1: Audit all callers (1-2 hours)

- Identify every file that still references old fields
- Categorize by crate and complexity
- Prioritize: core (engine) → protocol → agent → CLI → server → app

### Phase 2: Fix engine internals (core types)

- `crates/engine/src/types.rs`: Update `CompletionRequestBuilder::build()`
  to use `init_state`/`state_slot` (currently uses stub defaults)
- Remove stub fields from builder initializers
- Ensure the builder construction is complete and correct

### Phase 3: Fix agent validation modules

- `crates/agent/src/validation/*.rs`: Update `CompletionRequest` literals
  to use `init_state`/`state_slot` instead of `session`/`preserve_state`
- Remove `.system()` builder calls — the system text should be part of
  the raw prompt, not a separate field
- Fix intent.rs, planner.rs, summarizer.rs, wiki.rs, brainstorm.rs, inference.rs

### Phase 4: Fix CLI and LSP

- `crates/cli/src/lsp.rs`: Update FIM bake calls to use new API
- `crates/cli/src/cmd/story.rs`: Use `init_state`/`state_slot` where needed
- Remove stub field references from LSP completion code

### Phase 5: Fix server (HTTP API)

- `crates/server/src/routes.rs`: Update `Bake` route handler to use
  the new `bake` method signature instead of `bake_state`
- If backward compatibility is still needed, keep a thin wrapper
  but mark it `#[deprecated]`

### Phase 6: Cleanup protocol

- Remove `session` from `OpenAiCompletionRequest` in protocol
- Ensure `from_engine` and `into_engine` handle the migration cleanly

### Phase 7: Remove stub fields from `CompletionRequest`

- After all callers migrated, remove the deprecated stub fields
- Also remove `bake_state` from `ModelBackend` — backends can implement
  their own compatibility if needed via the new `bake` method

### Phase 8: Final verification

- Run full workspace check
- Run all tests (`cargo test --workspace`)
- Run manual E2E story pipeline to confirm end-to-end flow works

## Timeline

| Phase | Est. Time | Status |
|-------|-----------|--------|
| 1: Audit | 1-2 hrs | ⏳ Pending |
| 2: Engine internals | 30-60 min | ⏳ Pending |
| 3: Agent validation | 1-2 hrs | ⏳ Pending |
| 4: CLI/LSP | 30-60 min | ⏳ Pending |
| 5: Server | 30-60 min | ⏳ Pending |
| 6: Protocol cleanup | 30 min | ⏳ Pending |
| 7: Stub removal | 30 min | ⏳ Pending |
| 8: Verification | 1 hr | ⏳ Pending |

**Key decision**: Should I proceed with the phased migration approach
above, or would you prefer a different strategy? The goal is to keep
the codebase compiling at every step while systematically removing the
temporary stubs we added to restore compilation.
