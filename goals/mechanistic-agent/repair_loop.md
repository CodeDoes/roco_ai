# Repair Loop

Intent: Infer → Engine → Infer — grammar-validate every model output; run a structure oracle on produced artifacts; on failure, retry with tightened parameters (lower temperature, truncated length). Bounded retries, then fallback to a safe mode.

Sub-goals:
- Grammar validation: every model output must parse against its task grammar
- Structure oracle: validate file-level structure (wiki has required sections, chapters have correct format)
- Param tightening: on failure, reduce temperature, truncate length, prepend concision prompt
- Bounded retries: configurable MAX_RETRIES before fallback
- Fallback trigger: retry exhaustion → route to fallback mode via fallback_chains

Reference: `ksr/spec.md` — "Classic-logic repair loop (classical controller)". The three gates (grammar, schema/vocab, actions) form the validation chain.
