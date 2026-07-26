# RFC 0012: Automated Code Refactoring Harness
Status: Speculative

## Refactoring Loop Specs
1. Agent reads source file via `Sandbox::read()`.
2. Passes code + instructions to `ModelBackend`.
3. Validates output via `Verifier` (syntax check / dry parse).
4. On failure: `rollback()` restores original file version from workspace timeline.
5. On success: writes updated file via `Sandbox::write()`.
