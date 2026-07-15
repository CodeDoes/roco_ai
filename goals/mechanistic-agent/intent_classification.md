# Intent Classification

Intent: Classify user input against known routes to select the correct mode. Output is a structured intent (route name, confidence, goal, params). Low-confidence results route to `justChatting` as fallback. The model call for classification is grammar-constrained to the intent schema.

Reference: `mechanist_agent/idea.md` — `intent` schema with `route`, `confidence` (0..1), `goal`, `params`. Rules R21–R22 route on confidence thresholds.
