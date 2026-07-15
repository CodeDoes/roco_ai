# State-Mounted Instructions

Intent: System instructions are keyed by a hash of their content and mounted at session start — not prepended to every prompt. Switching modes swaps the mounted instruction set. This keeps the prompt lean and the system persona explicit and cacheable.

Reference: `ksr/spec.md` — "System instruction is state-mounted, not in-prompt (cache key: `sha256_hex(content_used_to_generate_state_tune)`)." `roco_ai` session Phase 1 state pool supports this via `AnyState::back()`/`load()`.
