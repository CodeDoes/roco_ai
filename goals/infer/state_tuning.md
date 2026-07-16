# What Would State-Tuning Enable

## Concept

"State-tuning" means shaping the model's recurrent state — not just the prompt text, but the actual RWKV hidden state — to guide output. This is distinct from both prompt engineering (text only) and grammar constraints (token-level enforcement).

The hypothesis: if the model's internal state is biased toward producing structured output (via well-crafted examples that were processed as prior turns), the contamination problem reduces enough that lightweight post-processing suffices.

## Current Evidence

### What Worked (in eval)

| Strategy | Result |
|----------|--------|
| Grammar-constrained (old schoolmarm/BNF) | ❌ Garbage — PUA byte mapping broke token matching |
| Unconstrained + state-tuning + post-processing | ✅ Perfect JSON in markdown fences, 1.6s/43 tok |
| Unconstrained (no examples) | ❌ `<>` leakage, think-tag contamination |

Key observation: the 2.9B model *can* produce valid JSON when given enough structure in the prompt. The problem is reliability — when it fails, it fails hard (open `<think>` tags, truncated JSON, structural artifacts).

### What Could Work Better

If state-tuning were made reliable (through better example selection, multi-shot priming, or temperature scheduling):

1. **Lower latency** — no grammar engine overhead per-token (kbnf's `mask_logits` is not free, though it's fast)
2. **More natural prose** — grammar-free output avoids the "stiff" artifact of strict token constraints, especially for chapter narrative
3. **Graceful degradation** — post-processing can salvage slightly malformed output (extra spaces, occasional markdown) where a grammar would force resampling
4. **No compilation conflicts** — no `kbnf` + `web-rwkv` type overflow to work around

## Why Grammar-First Still Wins

State-tuning failed the reliability bar for production:

1. **Contamination is non-deterministic** — the same prompt produces clean JSON on one run and `<think>` garbage on the next. Temperature doesn't explain it; the model's state has varying attractors.
2. **Post-processing is a lie** — stripping unclosed think tags doesn't fix the structural error in the output. The model produced `{ "name": "Bob"` then leaked `<think>I should close the brace` and the JSON is lost.
3. **Scalability** — every pipeline stage needs its own prompt engineering, example format, and post-processing rules. Grammars are declarative and composable.
4. **The 2.9B model is undertrained** — its pre-training state doesn't have robust JSON generation attractors. Grammar constraints fix this at the sampling layer, not the training layer.

## What State-Tuning IS Good For

Despite grammar-first being the primary approach, state-tuning has genuine use cases:

1. **Speed comparison baseline** — when evaluating a new grammar engine, state-tuned output provides the latency ceiling (no constraint overhead)
2. **Narrative prose** — chapter text generation may benefit from state-tuning because prose doesn't have a strict schema. The `chapter_prose` grammar should be a light constraint (minimum structure) rather than a full BNF.
3. **Hybrid approach** — use state-tuning for the first N tokens (warm up), then switch to grammar constraint once the model is in a known output mode. This hasn't been evaluated.
4. **Diagnostics** — if grammar-constrained generation fails (no tokens match), state-tuned bypass provides degraded-but-functional output

## Current Status

`StateTunedStrategy` in `crates/grammar/src/strategies.rs` implements:
- Empty grammar (no constraint)
- Markdown fence stripping (````json ... ````)
- Whitespace normalization
- `serde_json::from_str` with error reporting

It's useful as a fallback and as a comparison baseline in eval runs. It is NOT the primary strategy for production use.

## Open Questions

1. Can we measure state-tuning stability? E.g., run the same prompt 20 times, measure contamination rate.
2. Is there a threshold model size where state-tuning becomes sufficient? The g1h 2.9B may simply be too small.
3. Does temperature interact with state-tuning attractors? We observed contamination at all temperatures, but systematic measurement is missing.
4. Can we combine both: state-tune to prime, then lock with grammar for the final pass?

## See Also

- [why_bnf.md](why_bnf.md) — the case for grammar-first
- `crates/grammar/src/strategies.rs` — `StateTunedStrategy` implementation
- `crates/cli/examples/strategy_comparison.rs` — eval runner that compares state-tuned against grammar strategies
