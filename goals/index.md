# RoCo AI Goals

Roadmap for turning the local RWKV-7 inference engine into a full agent product.

Layers are listed in **prerequisite order** — each layer depends on the ones
above it. Within each layer, the files in its `index.md` are ordered the same
way (earlier = dependency of later).

## Layers (in order)

1. **infer** — the inference engine (model, quant, state, decoding, structured output)
2. **message** — chat protocol (instructions, formatting, tool calls, chat CLI)
3. **workspace** — the environment the agent acts in
4. **agent** — the autonomous agent loop and its capabilities
5. **agent_chat** — persistent workspace or folder-bound agent sessions
6. **browser_use** — driving a real browser
7. **testing** — eval harness, oracles, regression gates
8. **coder** — *(future)* the agent's own develop/test/lint loop in a controlled sandbox

Each folder contains an `index.md` listing its goals in dependency order.
