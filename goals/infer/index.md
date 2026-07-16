# Goals: infer

## What We Learned from Live Generation

Live multi-stage story pipeline runs on undertrained RWKV models (1B–2.9B) validated a core principle:

**Grammar-constrained decoding is non-negotiable for production use.** Free-form prompting produces systematic contamination (`<think>` tag leakage, planning text, meta-commentary) that no system prompt or temperature adjustment can eliminate. When output must satisfy a BNF grammar, the sampler rejects non-conforming tokens at every step — contamination literally cannot occur.

**Every stage needs its own domain grammar.** Plan and tool grammars are necessary but not sufficient. Story outline, wiki, chapter prose, validation reports, and synopsis all require dedicated grammars. Without them, the pipeline falls back to pre-fill workarounds (`<thinking>plan</thinking>` before prompt) which are explicit signals that domain grammars are still needed.

**Post-processing is a last resort.** Regex-based think block stripping is fragile because models often never close their tags. Architecture should prevent the problem at the sampling layer, not clean up after it.

See: [why_bnf.md](why_bnf.md) (overview), [gbnf.md](gbnf.md), [structured_output.md](structured_output.md), [thinking.md](thinking.md), [state_tuning.md](state_tuning.md) for detailed learnings.

## Goal Prerequisites

Prerequisite order (top to bottom):

1. **raw_model** — loading the raw SafeTensors / GGUF model weights
2. **tokenization** — the RWKV tokenizer (vocab, BPE encoding)
3. **quantize_model** — NF4 / Int8 quantization to fit VRAM
4. **inference** — the core forward-pass / generate loop
5. **streaming** — token-by-token streaming output via callback
6. **gbnf** — GBNF grammar-constrained decoding
7. **structured_output** — JSON Schema → GBNF conversion
8. **structured_output_objects** — object/array support in schema→GBNF
9. **thinking** — chain-of-thought / `<think>` extraction
10. **state_saving** — saving RWKV recurrent state to disk/RAM
11. **state_loading** — loading a saved state back into the model
12. **state_mixing** — multi-session state pool with LRU eviction
13. **interrupt_inference** — cancelling mid-generation (Ctrl+C)
14. **continue_inference** — resuming / continuing from a partial response
15. **gguf_st_converter** — GGUF → SafeTensors format converter for smaller models


maintenance plus one blocked fix.

## Lessons Learned from Live Generation

Multi-stage story pipeline runs on undertrained RWKV models (1B–2.9B) validated the **grammar-first principle**: free-form prompting produces systematic contamination (`<thinking>` tag leakage, planning text, meta-commentary) that no prompt or temperature adjustment can eliminate. Grammar-constrained decoding rejects non-conforming tokens at every sampling step — contamination literally cannot occur.

**Key findings:**
- Every stage needs its own domain grammar (outline, wiki, chapter, validation, synopsis)
- Post-processing (regex stripping, pre-fill workarounds) is a last resort signaling where grammars are still needed
- Pre-fill pattern (`<thinking>plan</thinking>` before prompt) works as interim measure for undertrained models
- Architecture decision: **every model call must go through a BNF grammar** — this is non-negotiable for production use

See: `goals/infer/thinking.md`, `goals/infer/gbnf.md`, `goals/mechanistic-agent/task_grammars.md` for full details.

## Prerequisite Order (mirrors the product layer)

1. **raw_model** ✅ done. Keep `Loader::info` auto-detect healthy.
2. **tokenization** ✅ done.
3. **quantize_model** ✅ done (NF4/Int8 from on-disk size). Self-directed:
   add an eval/unit case asserting the chosen quant plan for a known file size.
4. **inference** ✅ done.
5. **streaming** ✅ done.
6. **gbnf** ✅ done.
7. **structured_output** ✅ done.
8. **structured_output_objects** ✅ done.
9. **thinking** ✅ done.
10. **state_saving** ✅ done.
11. **state_loading** ✅ done.
12. **state_mixing** ✅ done (Phase 1 LRU pool).
13. **interrupt_inference** ✅ done.
14. **continue_inference** ✅ done.
15. **gguf_st_converter** 🔴 **blocked.** The 0.1B/1.5B GGUF→ST converter
    drops 3-D / matrix shapes (`a0/k_a/k_k/v0/w0/x_*` need `[1,1,emb]`;
    `r_k` needs `(clock_count, head_dim)`). My self-directed action: attempt
    the upstream patch when convenient; until then, document the blocker
    clearly and keep the 2.9B path as the supported baseline.

**Next self-directed action:** The infer layer is complete for the 2.9B path, but live testing revealed a critical gap: **per-handler BNF grammars are missing from the story pipeline**. The next priority is wiring `BnfConstraint` into each story stage handler (outline, wiki, chapters ×3, validation, synopsis) so the grammar-first principle holds end-to-end. Until then, the pipeline uses pre-fill workarounds as interim measures.

Only pick up the GGUF→ST fix or add quant/state eval coverage after the grammar gap is closed.
