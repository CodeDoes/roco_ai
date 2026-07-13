# RoCo AI Goals

Roadmap for turning the local RWKV-7 inference engine into a full agent product.
Layers are ordered by dependency; **the numeric prefix on every file is the
order it should be handled in** (lower = sooner).

- `1_infer/` — the inference engine (model, quant, state, decoding)
- `2_message/` — chat protocol (instructions, formatting, tool calls)
- `3_workspace/` — the environment the agent acts in
- `4_agent/` — the autonomous agent loop and its capabilities
- `5_browser_use/` — driving a real browser

Within each folder, files are numbered `NN_*` in **prerequisite order** — a file's
dependencies always come before it (e.g. `tokenization` precedes `inference`;
`tool_catelogue` precedes `tool_calling`; the core `tool_execution_loop` is
foundational). Files may grow a `User:` section with notes/constraints added
during planning.
