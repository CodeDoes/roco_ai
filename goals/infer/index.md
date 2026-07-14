# Goals: infer

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
