# GGUF → ST Converter (0.1B / 1.5B)

Intent: Fix `scripts/gguf_to_st_converter/` so the 0.1B and 1.5B RWKV-7 checkpoints convert to SafeTensors in the layout web-rwkv expects (`a0/k_a/k_k/v0/w0/x_*` as `[1,1,emb]`, `r_k` as `(clock_count,head_dim)`). Unblocks those checkpoints end-to-end — today only the 2.9B works on disk.

User: Upstream fix reads `rwkv7.embedding_length` + `rwkv7.wkv.head_size` from GGUF metadata. Known blocker — see PROGRESS.md "Things we tried that didn't work".
