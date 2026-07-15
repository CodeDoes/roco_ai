# Intent Classification

Intent: Classify user input against known routes to select the correct mode. Output is a structured intent (route name, confidence, goal, params). Low-confidence results route to `justChatting` as fallback. The model call for classification is grammar-constrained to the intent schema.

Sub-goals:
- Intent schema: BNF-constrained output shape (route, confidence, goal, params)
- Route matching: map intent route names to available modes
- Confidence threshold: configurable FALLBACK_THRESHOLD for safe fallback
- Mixed intent: chain multiple modes when confidence is split

Reference: `mechanist_agent/idea.md` — `intent` schema with `route`, `confidence` (0..1), `goal`, `params`. Rules R21–R22 route on confidence thresholds.
