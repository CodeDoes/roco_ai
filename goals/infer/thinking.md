# Thinking

## Goal
Support controlled reasoning modes where the model outputs structured thinking before its final answer.

## What We Learned (from live generation)
### The `think>` Tag Problem
Undertrained base RWKV models (1B–2.9B g1h) **consistently leak** `<think>...` meta-commentary into output:
- System prompts saying "no thinking" have **zero effect**
- Temperature decay (0.6→0.3) doesn't stop it either
- Every stage of multi-step pipelines (outline → wiki → chapters → validate → synopsis) gets contaminated
- The model writes planning text like `"Okay let me plan..."`, `"First I need to..."`, `"We need to write Chapter 1..."`
- These are NOT proper chain-of-thought — they're training artifacts from the base tokenizer/model combo

### Workaround 1: Pre-fill Think Block
Add `\nthi nk>\nplan_content\n</think>` **before** the actual request prompt. This:
- Makes the model believe it has already done its thinking
- The model continues directly into the requested content
- Then strip the wrapper tags as cleanup
- Works because undertrained RWKV completes whatever follows a `<think>` marker
- May need newlines/spaces around the tags depending on model size (undertraining artifact)

### Workaround 2: Post-generation Stripping
Find-and-remove `<think>`/`</think>` markers from output, keeping surrounding prose.
**Problem:** Undertrained models often never close their think blocks — content bleeds past them.
Single-pass regex replace removes all marker instances; no pairing logic needed.

### The Real Fix: BNF Grammar Constraints
**GBNF-constrained decoding is the correct solution.** When the model's output must satisfy a grammar:
- It literally cannot emit unconstrained meta-commentary — the sampler rejects it
- No post-processing, no retries, no fallbacks, no heuristic workarounds
- Output is structurally guaranteed at the point of generation
- `BnfConstraint` (`crates/grammar/src/bnf.rs`) with `bnf_sampler` is the primary engine

**Priority:** Every stage handler should use its own domain-specific BNF grammar rather than free-form prompting. This eliminates contamination at the source.

## Open Questions
- Can we synthesize BNF grammars automatically from task descriptions?
- Does grammar-constrained decoding improve coherence across multi-step pipelines?
- Should the story pipeline become a `roco story` CLI subcommand using grammar-constrained stages?
