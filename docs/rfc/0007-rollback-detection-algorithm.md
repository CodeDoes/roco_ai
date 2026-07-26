# RFC 0007: Stuck-State Detection and Rollback Algorithm
Status: Implemented

## Algorithm Specification
1. Agent runs `step()`. Output evaluated via `Verifier::verify()`.
2. If `verify()` succeeds -> state checkpoint updated, loop completes.
3. If `verify()` fails:
   - `state.attempts` incremented.
   - `rollback()` invoked: resets state to previous checkpoint.
   - History logs rollback event with failure details.
4. If `attempts >= max_retries` (default 3):
   - Loop exits.
   - `StackResult.success = false`, `rollback_count` recorded.
   - System flags persistent failure for harness intervention.
