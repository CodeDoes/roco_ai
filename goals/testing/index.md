# Goals: testing

Prerequisite order (top to bottom):

1. **eval_harness** — the eval suite, oracle management, and regression gates;
   `roco eval` runs `EvalCase` records against any `ModelBackend`, saves snapshots,
   and `roco bless` promotes snapshots to blessed oracles
