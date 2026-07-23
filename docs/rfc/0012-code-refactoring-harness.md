# RFC 0012: Automated Legacy Code Refactoring Harness
Status: Speculative
Agent selects file from workspace, reads content via Sandbox, passes through MockBackend simulating refactoring instructions. Verifier checks output syntax validity by attempting parse (if language parser available). Rollback restores original file from workspace timeline if verification fails.
