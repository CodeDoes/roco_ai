# Self-Directed Goals: testing

Reflection of [`goals/testing/index.md`](../../goals/testing/index.md). The
eval harness is done. My self-directed work is keeping the oracle/snapshot
gate honest as new layers add behavior.

Prerequisite order (mirrors the product layer):

1. **eval_harness** — ✅ done (`roco_engine::eval` + `eval_suite` example,
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
