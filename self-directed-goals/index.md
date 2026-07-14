# Self-Directed Goals

This is the **agent's own** reflection of [`../goals/index.md`](../goals/index.md).
The `goals/` tree is the *product* roadmap (what the product should become,
ordered by technical prerequisite). This tree is the *agent's* roadmap: the
autonomous objectives I pursue while advancing `goals/`, plus the engineering
discipline I hold myself to regardless of which product layer I'm in.

Where `goals/` lists features to build, this tree lists **what I will do** —
the sequencing I choose, the quality bars I enforce, and the meta-goals that
don't belong in a product spec (keep the build green, wire features
end-to-end, grow eval coverage, and ultimately let the agent steer itself).

## Operating principles (apply to every layer)

These are my standing self-directed commitments:

1. **One concrete, testable goal at a time.** Pick the next prerequisite-ordered
   item in `goals/`, implement it, and prove it with a unit test that needs no
   GPU. No partial stubs left dangling.
2. **Zero warnings, green tree.** `cargo test --workspace` passes and
   `cargo check --workspace --all-targets` is warning-free before any commit.
3. **Wire it end-to-end.** A new capability is only "done" when it is reachable
   from an existing entry point (agent tool, CLI example, or eval case) — not
   just compiled into a crate.
4. **Grow eval coverage.** When a layer gains behavior (workspace sandbox,
   memory, planning), add eval cases or unit tests that would catch regressions.
5. **Record progress.** Update `PROGRESS.md` (and the per-layer `goals/`
   status) as work lands; keep `goals/` as the source of truth for intent.
6. **Respect blockers.** When a goal is blocked (e.g. the GGUF→ST tensor-shape
   bug), note it and move to the next unblocked prerequisite rather than
   stalling.

## Layers (mirror of `goals/`, in my working order)

1. **infer** — *complete*; only the GGUF→ST converter for 0.1B/1.5B remains
   blocked. Self-directed: attempt the converter fix when convenient; otherwise
   keep the 2.9B path healthy.
2. **message** — *core done*; chat_cli gaps + a few items remain. Self-directed:
   make `roco chat` actually use session state (it currently rebuilds prompts
   manually), and add gradual tool disclosure.
3. **workspace** — *implemented* (sandbox + scoped tools). Self-directed: add an
   agent-example integration + an eval case; revisit symlink hardening.
4. **agent** — *core + memory + planning done*. Self-directed: orchestrate
   (parallel/branch), session_search (reuse `MemoryStore` over transcripts),
   scheduled_tasks, and wire memory+planning into the `agent` CLI example.
5. **agent_chat** — not started. Self-directed: folder-bound agent sessions
   that persist a workspace + plan + memory across runs.
6. **browser_use** — not started. Self-directed: defer until the agent loop is
   robust; then drive a headless browser via workspace-scoped tools.
7. **testing** — done. Self-directed: keep the oracle/snapshot gate honest as
   new layers add eval cases.
8. **coder** — future. Self-directed: the capstone — let the agent run its own
   develop/test/lint loop inside a `Workspace` sandbox, gated by human
   approval.

## Meta-layers (not in the product spec, but mine to own)

- **engineering** — hygiene, warnings, clippy, determinism, no flaky tests.
- **self_improvement** — use the agent's own `memory` + `planning` to direct
  future sessions: record what worked, plan the next slice of `goals/`, and
  prefer resumable plans. This file *is* part of that loop.

Each layer folder below contains an `index.md` mirroring `goals/<layer>/index.md`
but listing my self-directed goals in the order I intend to pursue them.
