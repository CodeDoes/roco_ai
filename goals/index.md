# RoCo AI Goals

Roadmap for turning the local RWKV-7 inference engine into a full agent product.

## Core Architectural Principle: Grammar-First

**Every model call must go through a BNF grammar.** Live testing on undertrained RWKV-7 models (1B–2.9B) proved this is non-negotiable:

- Free-form prompting produces systematic contamination (`<thinking>` tag leakage, planning text, meta-commentary) that no prompt or temperature adjustment can eliminate
- Grammar-constrained decoding rejects non-conforming tokens at every sampling step — contamination cannot occur
- Every stage needs its own domain grammar (outline, wiki, chapter, validation, synopsis) — plan and tool grammars are necessary but not sufficient
- Post-processing (regex stripping, pre-fill workarounds) is a last resort, signaling where domain grammars are still needed

See: `goals/infer/thinking.md`, `goals/infer/gbnf.md`, `goals/mechanistic-agent/task_grammars.md` for detailed learnings.

## Operating Principles (apply to every layer)

These are my standing self-directed commitments:

1. **One concrete, testable goal at a time.** Pick the next prerequisite-ordered item in `goals/`, implement it, and prove it with a unit test that needs no GPU. No partial stubs left dangling.
2. **Zero warnings, green tree.** `cargo test --workspace` passes and `cargo check --workspace --all-targets` is warning-free before any commit.
3. **Wire it end-to-end.** A new capability is only "done" when it is reachable from an existing entry point (agent tool, CLI example, or eval case) — not just compiled into a crate.
4. **Grow eval coverage.** When a layer gains behavior (workspace sandbox, memory, planning), add eval cases or unit tests that would catch regressions.
5. **Record progress.** Update `PROGRESS.md` (and the per-layer `goals/` status) as work lands; keep `goals/` as the source of truth for intent.
6. **Respect blockers.** When a goal is blocked (e.g. the GGUF→ST tensor-shape bug), note it and move to the next unblocked prerequisite rather than stalling.
7. **Grammar-first, always.** Every model call goes through a BNF grammar. Free-form prompting on undertrained RWKV models produces systematic contamination (`<thinking>` tag leakage, meta-commentary) that no prompt or temperature adjustment can eliminate. Grammar-constrained decoding rejects non-conforming tokens at every sampling step — contamination cannot occur. Post-processing (regex stripping, pre-fill workarounds) signals where domain-specific grammars are still needed.

## Layers

Layers are listed in **prerequisite order** — each layer depends on the ones above it. Within each layer, the files in its `index.md` are ordered the same way (earlier = dependency of later).

## Product Roadmap (what to build)

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

## Per-Layer Status (what's actually done + what I'm working on next)

| Layer | Status | Next self-directed action |
|---|---|---|
| **infer** | ✅ 2.9B path complete; GGUF→ST blocked | close per-handler grammar gap before quant evals |
| **message** | ✅ core done | wire session state into `roco chat`; add gradual tool disclosure |
| **workspace** | ✅ sandbox + scoped tools built | integration test + eval case; revisit symlink hardening |
| **agent** | ✅ core + memory + planning | orchestrate parallel/branch; session_search; wire into CLI example |
| **mechanistic-agent** | 🟡 core done, grammar gap | wire BnfConstraint into each story stage handler |
| **agent_chat** | ✅ folder-bound sessions working | let resumed session reuse prior plan |
| **browser_use** | ⬜ not started | defer until agent loop is robust; then drive headless browser via tools |
| **testing** | ✅ eval harness, oracles, snapshots | keep oracle/snapshot gate honest as new layers evolve |
| **coder** | ⬜ future capstone | develop/test/lint loop inside Workspace sandbox, gated by human approval |

## Meta-layers (not in the product spec, but mine to own)

- **engineering** — hygiene, warnings, clippy, determinism, no flaky tests.
- **self_improvement** — use the agent's own `memory` + `planning` to direct
  future sessions: record what worked, plan the next slice of `goals/`, and
  prefer resumable plans. This file *is* part of that loop.
