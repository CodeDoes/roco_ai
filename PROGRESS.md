# Pipeline Fixes + Eval-First Direction

## Fixed Bugs

| Bug | Root Cause | Fix |
|---|---|---|
| Session state lost between chapters | Mapped `session` → `state_id`+`save_as` only when `preserve_state=true` | Map both always — old code saved unconditionally when `session` was set |
| `feed_eos` no-op | New code just reset in-memory state, didn't touch pool | Restored old behavior: load from pool, feed token 0 (EOS), save back to pool |
| Mock backend silent | Matching on `req.system` which is now empty | Match on `req.prompt` with unique phase identifiers |
| Control chars break JSON | Model emits NUL/control chars | Added `strip_control_chars()` in `clean_json_output()` |
| Outline truncated at 800 tokens | `repair_truncated_json` wasn't being called | Now closes unclosed brackets |
| Outline text data flow | Handler wrote full markdown but returned short summary without chapter details | Main pipeline reads the written file back |
| `--fix` workspace discovery | Created fresh empty workspace instead of finding latest | `--fix` implies `--resume` |
| Validator state bleed | Cross-chapter persistence in validator session | `feed_eos(SESSION_VALIDATOR)` before each validation |
| Retry loop accepted without re-validate | Revised once then accepted unconditionally | Loop validates after each revision, up to 3 retries |

## Architecture Change: inferd Only Receives Raw Text

**Before:** actor formatted `System:/User:/Assistant:` wrappers, handled `<think>`
suppression, and managed implicit session-based state.

**After:** actor receives raw text only. `CompleteReq` has `state_id`/`save_as` for
explicit state management. `Bake` is a separate message (feed text, save state).
`FeedEos` loads from pool, feeds EOS token, saves back. Gateway owns formatting.

## Eval-First Direction

The prose fallback parsers (`prose_to_outline`, `prose_to_wiki`, `prose_to_chapter`,
`prose_to_validation`, `prose_to_synopsis`) are **wrong.** They were added because
I assumed the 2.9B model can't output JSON. But that assumption was never validated
with evals on a correctly-functioning pipeline.

The correct order:
1. **Fix the system** (done — bugs above)
2. **Run evals** to prove the model CAN output JSON given the right prompts, baking,
   grammar, and temperature. Evaluate each phase separately.
3. **If evals pass**, remove the prose fallback parsers. JSON parse failure = phase
   failure = fix the system, not silently fall back.
4. **If evals fail**, tune prompts/baking/grammar/temperature until they pass.

Existing eval infrastructure:
- `crates/engine/src/story_evals.rs` — per-stage evals for outline, wiki, chapter,
  validation, revision
- `crates/cli/examples/format_eval.rs` — multi-format eval with state-tune delta
- `evals/` — shell scripts + results

The existing format_eval results show format_ok=false for everything, but those
were run on the OLD buggy system (before session mapping fix, before feed_eos fix).
Post-fix evals need to be run.

## Remaining Cleanup

- [ ] Run post-fix format_eval against real model to verify JSON output capability
- [ ] Run post-fix story_evals to validate each pipeline phase
- [ ] **Remove prose fallback parsers** if evals prove model can output JSON
- [ ] Collapse harness crate's 10 × identical `Agent` structs into single `MockAgent`
- [ ] Migrate `agent-journal.md` from Markdown to JSONL
- [ ] Rename `StrategyKind`/`StrategySelector` if the name is confusing
