# RoCo AI Goals

Roadmap for turning the local RWKV-7 inference engine into a full agent product.

## Core Architectural Principle: Grammar-First

**Every model call must go through a BNF grammar.** Live testing on undertrained RWKV-7 models (1B–2.9B) proved this is non-negotiable:

- Free-form prompting produces systematic contamination (`<thinking>` tag leakage, planning text, meta-commentary) that no prompt or temperature adjustment can eliminate
- Grammar-constrained decoding rejects non-conforming tokens at every sampling step — contamination cannot occur
- Every stage needs its own domain grammar (outline, wiki, chapter, validation, synopsis) — plan and tool grammars are necessary but not sufficient
- Post-processing (regex stripping, pre-fill workarounds) is a last resort, signaling where domain grammars are still needed

See: `goals/infer/thinking.md`, `goals/infer/gbnf.md`, `goals/mechanistic-agent/task_grammars.md` for detailed learnings.

## Layers

Layers are listed in **prerequisite order** — each layer depends on the ones
above it. Within each layer, the files in its `index.md` are ordered the same
way (earlier = dependency of later).

## Layers (in order)

1. **infer** — the inference engine (model, quant, state, decoding, structured output)
2. **message** — chat protocol (instructions, formatting, tool calls, chat CLI)
3. **workspace** — the environment the agent acts in
4. **agent** — the autonomous agent loop and its capabilities
5. **mechanistic-agent** — code-driven controller + router plugin; model is a subroutine, control flow stays in classic code
6. **agent_chat** — persistent workspace or folder-bound agent sessions
7. **browser_use** — driving a real browser
8. **testing** — eval harness, oracles, regression gates
9. **coder** — *(future)* the agent's own develop/test/lint loop in a controlled sandbox

Each folder contains an `index.md` listing its goals in dependency order.
