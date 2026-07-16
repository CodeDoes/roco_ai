# Goals: testing

## Grammar-First Principle

The testing layer validates grammar-constrained generation end-to-end. Every eval case verifies that model output satisfies BNF grammars — not just that it matches expected text. Grammar-constrained decoding rejects non-conforming tokens at every sampling step, making contamination impossible (see `goals/infer/thinking.md`).

## Prerequisites

Prerequisite order (top to bottom):

1. **eval_harness** — the eval suite, oracle management, and regression gates;
   `roco eval` runs `EvalCase` records against any `ModelBackend`, saves snapshots,
   and `roco bless` promotes snapshots to blessed oracles


## Status & Self-Directed Actions

gate honest as new layers add behavior.

Prerequisite order (mirrors the product layer):

1. **eval_harness** ✅ done (`roco_engine::eval` + `eval_suite` example,
   snapshot/bless workflow).

**Self-directed additions (my discipline, not a product sub-goal):**
- Every new layer that changes model-visible behavior (workspace tools,
  memory, planning) gets an eval case or unit test that would catch a
  regression — I hold myself to this before declaring a goal done.
- Keep `cargo test --workspace` green and `cargo check --workspace
  --all-targets` warning-free as the standing gate.
- When the oracle drifts because the model genuinely improved, run `roco
  bless` deliberately — never to mask a bug.

**Next self-directed action:** none urgent; this is a standing commitment
enforced while working on other layers.
