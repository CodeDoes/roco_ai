# RFC 0002: Full Stack Integration Loop
Status: Implemented (Harness Execution)

## Integration Flow
`StackRunner` connects input -> domain selection -> agent -> backend execution -> output verification -> state rollback -> result compilation.

## Execution Pipeline
1. `run_all()` / `run_with_sandbox_and_verifier()`
2. Passes input through `DomainHarness`
3. Checks output with `Verifier`
4. On failure: invokes `rollback()`, increments attempt counter, retries
5. Returns `StackResult` with final state, attempt count, and success boolean.
