# Fallback Chains

Intent: Modes declare fallback chains — if a mode's confidence is below threshold, if a handler fails, or if the repair loop exhausts retries, the controller routes to the next mode in the chain. The terminal fallback is always `justChatting` (safe mode, no tools, clarify instead of hallucinate).
