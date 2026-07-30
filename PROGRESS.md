# Current Focus: Chapter Content Quality

## Status After Full E2E Sweep (2026-07-30, 3rd run)

### ✅ Fixed Bugs

| Bug | Fix | Result |
|---|---|---|
| `feed_eos` was a silent no-op | Added `feed_eos` to `RemoteBackend` (HTTP POST to `/sessions/{id}/eos`) + fixed `TokioBackend` to delegate to inner | Chapters are no longer byte-identical |
| Full previous chapter text in prompts | Changed to brief summary reference | Prompt sizes stay small (~571 vs 13963); no more anchoring on previous chapter opening |

### ✅ What Works Now
- All 3 chapters have **unique content** (different MD5 hashes)
- Prompt sizes stay reasonable (482 → 571 → 571 tokens)
- Pipeline completes in ~4 min (vs ~8 min before)
- 8 files produced, story published
- 1002 tests pass, 0 warnings

### ❌ Remaining Issues (Model Capability, not Code)
- Chapters **don't follow the outline** — model generates its own stories
- Chapters **don't connect** — each is a different unrelated scene
- Story **doesn't match the premise** — "A robot learns to paint" → unrelated sci-fi
- **Validation false-positive** — passes chapters that don't match the outline

### Root Cause
The 2.9B model is too small to follow complex narrative instructions. It generates prose from its training data patterns, ignoring the detailed outline provided.

### What Would Help
- **Better prompts**: shorter, more directive, with stronger JSON enforcement
- **Lower temperature** (0.3-0.5) for more deterministic output
- **BNF grammar enforcement** currently blocked by mock backend compatibility (see below)
- **Larger model** would follow instructions better

### BNF Grammar Blocker
`StateTunedStrategy` returns empty grammar (no BNF). `SchemaStrategy` generates proper GBNF but breaks mock backend tests because `mock_random_walk_bnf` produces garbage with the limited mock vocabulary. Fixing this requires either:
- Making the mock backend's BNF walk produce valid JSON for real schemas (complex)
- Or adding grammar support to `StateTunedStrategy` only when the backend is not mock (no clean detection)

## UX/DX Issues Documented from E2E

1. **Silent 4-min model build** — No progress bar during V7 GPU kernel compilation
2. **No streaming output** — User sees nothing during 85s generation
3. **Gateway auto-start uses `cargo run`** — Compiles from source in dev mode
4. **Validation false-positive** — Passes chapters with unrelated content
5. **No `--mock` flag in help** — Added to code but not in CLI help text
6. **Outline/worldbuilding retries invisible** — User sees "✓ Complete" but doesn't know if retries happened
7. **No GPU pre-flight check** — Model loading fails silently only when user tries to generate
8. **Chapters don't follow outline** — Model limitation, but no warning is shown to user

## Next Actions
- [x] Fix `feed_eos` being a no-op
- [x] Fix prompt not including full previous chapter text
- [ ] Consider: Add BNF grammar to `StateTunedStrategy` (blocked by mock backend)
- [ ] Consider: Lower default temperature to 0.5 for more consistent output
- [ ] Consider: Better prompts that enforce outline adherence
