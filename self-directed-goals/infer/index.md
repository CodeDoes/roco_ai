# Self-Directed Goals: infer

Reflection of [`goals/infer/index.md`](../../goals/infer/index.md). The
inference engine is **complete** on the 2.9B path; my self-directed work here
is maintenance plus one blocked fix.

Prerequisite order (mirrors the product layer):

1. **raw_model** — ✅ done. Keep `Loader::info` auto-detect healthy.
2. **tokenization** — ✅ done.
3. **quantize_model** — ✅ done (NF4/Int8 from on-disk size). Self-directed:
   add an eval/unit case asserting the chosen quant plan for a known file size.
4. **inference** — ✅ done.
5. **streaming** — ✅ done.
6. **gbnf** — ✅ done.
7. **structured_output** — ✅ done.
8. **structured_output_objects** — ✅ done.
9. **thinking** — ✅ done.
10. **state_saving** — ✅ done.
11. **state_loading** — ✅ done.
12. **state_mixing** — ✅ done (Phase 1 LRU pool).
13. **interrupt_inference** — ✅ done.
14. **continue_inference** — ✅ done.
15. **gguf_st_converter** — 🔴 **blocked.** The 0.1B/1.5B GGUF→ST converter
    drops 3-D / matrix shapes (`a0/k_a/k_k/v0/w0/x_*` need `[1,1,emb]`;
    `r_k` needs `(clock_count, head_dim)`). My self-directed action: attempt
    the upstream patch when convenient; until then, document the blocker
    clearly and keep the 2.9B path as the supported baseline.

**Next self-directed action:** no urgent infer work — the 2.9B path is the
product baseline. Only pick this layer up to attempt the GGUF→ST fix or to
add quant/state eval coverage.
