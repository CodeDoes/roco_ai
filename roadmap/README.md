# Roadmap — RoCo AI

> This folder is the **living plan**. It replaces the old `goals/` scratchpad.
> Unlike `goals/`, this is not a feature checklist for the engine — it is the
> plan for the **human-facing experience**, and every item here must end in a
> tested, usable surface.

## How to use this folder

- `README.md` — this file: the current focus and the definition of done.
- `ux/` — the human-centric experience plan (layouts, flows, components).
- `progress.md` — what changed, when, and why. Append-only; the agent writes
  here after each meaningful change so the human can see trajectory without
  reading git diffs.
- `blocked.md` — open questions / things waiting on a decision (not a todo
  list, a parking lot).

Keep entries short. Link to code, not prose. The point is trajectory, not
comprehensive documentation.

## Current focus (2026-07-19)

**The engine is done and frozen. We are building the experience.**

The Rust core (`crates/inference`, `engine`, `grammar`, `bnf-engine`,
`agent`, `session`, `message`, `tools`, `workspace`) is correct and tested.
Do not churn it. New work goes into the **frontend** — currently migrating
from the untested webapps in `apps/` toward a gpui desktop app so the UI
lives in the same tested Rust tree as the engine.

The defect we are correcting: the human-centric *logic* exists and is tested
(`crates/agent/src/interaction.rs`, `story_direction.rs`, `commentary.rs`,
`chapter_steering.rs`) but the *surface* never exposed it. The webapps had
zero tests and did not surface accept / skip / stop / pause / commentary.

## Definition of Done (applies to EVERY feature)

A feature is NOT done when "it exists in a crate." It is done when ALL hold:

1. **Surface** — the human can reach it through the actual UI (not a CLI flag,
   not an example binary).
2. **Control** — if it affects the story, the human can accept / modify /
   skip / stop it, per the pace philosophy.
3. **Tested** — there is an automated test (unit for logic, integration/UI for
   the surface) proving a human can drive it. No test = not done.
4. **Reversible** — actions the human takes can be undone (VersionControl
   already exists in the engine; the UI must wire it).

If you cannot satisfy these, the task is partially done — say so explicitly,
do not mark it complete.
