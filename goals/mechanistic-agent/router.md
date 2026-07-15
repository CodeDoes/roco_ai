# Router

Intent: A static (type, domain) → handler dispatch table. Unknown pairs fail loud instead of letting the model improvise. Supports fallback/conditional routing via confidence thresholds.

Reference: `mindful/agent/engine.py` — `MechanisticAgent.run()` dispatches tasks from `plan["tasks"]` against a `handlers` dict keyed by `(type, domain)`.
