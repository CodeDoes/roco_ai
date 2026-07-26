# RFC 0014: Catastrophic Failure Mode Handling
Status: Safety Critical

## Failure Mitigation Matrix
- **Empty Model Response:** Immediate `Verifier` failure; triggers rollback.
- **Max Retries Exceeded:** Halts execution loop cleanly; sets `StackResult.success = false` without crashing process.
- **Path Escape Attempt:** `Sandbox` returns `Err("path escape blocked")` immediately.
- **Context Buffer Overflow:** LRU eviction policy drops oldest entry when `Context.memory > 100`.
