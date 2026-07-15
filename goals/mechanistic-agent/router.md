# Router

Intent: A static (type, domain) → handler dispatch table. Unknown pairs fail loud instead of letting the model improvise. Supports fallback/conditional routing via confidence thresholds.

Sub-goals:
- Dispatch table: map (type, domain) pairs to registered handlers
- Unknown pair handling: fail loud with a clear error, never let the model improvise
- Mode-scoped dispatch: each mode has its own dispatch table
- Conditional routing: confidence thresholds redirect between handlers/modes

Reference: `mindful/agent/engine.py` — `MechanisticAgent.run()` dispatches tasks from `plan["tasks"]` against a `handlers` dict keyed by `(type, domain)`.
