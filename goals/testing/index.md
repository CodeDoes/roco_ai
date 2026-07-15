# Goals: testing

## Grammar-First Principle

The testing layer validates grammar-constrained generation end-to-end. Every eval case verifies that model output satisfies BNF grammars — not just that it matches expected text. Grammar-constrained decoding rejects non-conforming tokens at every sampling step, making contamination impossible (see `goals/infer/thinking.md`).

## Prerequisites

Prerequisite order (top to bottom):

1. **eval_harness** — the eval suite, oracle management, and regression gates;
   `roco eval` runs `EvalCase` records against any `ModelBackend`, saves snapshots,
   and `roco bless` promotes snapshots to blessed oracles
