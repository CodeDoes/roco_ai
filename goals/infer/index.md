# Goals: infer

## What We Learned from Live Generation

Live multi-stage story pipeline runs on undertrained RWKV models (1B–2.9B) validated a core principle:

**Grammar-constrained decoding is non-negotiable for production use.** Free-form prompting produces systematic contamination (`<think>` tag leakage, planning text, meta-commentary) that no system prompt or temperature adjustment can eliminate. When output must satisfy a BNF grammar, the sampler rejects non-conforming tokens at every step — contamination literally cannot occur.

**Every stage needs its own domain grammar.** Plan and tool grammars are necessary but not sufficient. Story outline, wiki, chapter prose, validation reports, and synopsis all require dedicated grammars. Without them, the pipeline falls back to pre-fill workarounds (`<thinking>plan</thinking>` before prompt) which are explicit signals that domain grammars are still needed.

**Post-processing is a last resort.** Regex-based think block stripping is fragile because models often never close their tags. Architecture should prevent the problem at the sampling layer, not clean up after it.

See: [gbnf.md](gbnf.md), [structured_output.md](structured_output.md), [thinking.md](thinking.md) for detailed learnings.

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
