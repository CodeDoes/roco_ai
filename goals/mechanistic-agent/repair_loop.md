# Repair Loop

Intent: Infer → Engine → Infer — grammar-validate every model output; run a structure oracle on produced artifacts; on failure, retry with tightened parameters (lower temperature, truncated length). Bounded retries, then fallback to a safe mode.

Reference: `ksr/spec.md` — "Classic-logic repair loop (classical controller)". The three gates (grammar, schema/vocab, actions) form the validation chain.
