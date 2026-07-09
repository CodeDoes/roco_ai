# RoCo AI — Plan

## Where we are

✅ **Phase 0 complete** — Cargo workspace with 5 crates, compiles, all demos pass:

```
roco_ai/
├── crates/core/         ← roco_core  (engine, agent, memory, tools, vector, trace, ...)
├── crates/cli/          ← roco binary (demo harness, viz, eval)
├── crates/session/      ← roco_session (Engine, message queue, poll loop)
├── crates/workspace/    ← roco_workspace (managed filesystem)
├── crates/gateway/      ← stub (axum HTTP, Phase 3)
├── gui/                 ← Dioxus visualizer (excluded from workspace — GTK deps)
├── SPEC.md              ← architecture spec
├── PROGRESS.md          ← phase tracker
└── PLAN.md              ← this file
```

## Next: the debug loop

The SPEC says the whole point is replacing "screenshot + pass/fail" with
"structured, diffable execution history." The trace system exists in core but
is only exercised by `cargo run -- viz`. The next step is to make traces a
first-class citizen:

1. **Enrich the trace contract** — every agent path (decompose, verify,
   escalate, tool-call, budget-check) emits a `TraceEvent`. Currently some
   paths don't.
2. **Trace persistence** — every run writes a `.roco/traces/<id>.json` so you
   can replay it later without re-running the agent.
3. **Trace diff** — compare two traces to see where behavior diverged.

## Then: oRPC + napi-rs (SPEC Phase 2)

This is where the frontend comes in:

```
crates/napi/  (napi-rs addon)
  → calls roco_core::Orchestrator::run()
  → returns / streams TraceEvent[]

Next.js app
  → oRPC server procedure: runTask(req) → Trace
  → visualizer subscribes to trace stream
  → assistant-ui for chat interface
```

### Why this matters for debugging

| Before | After |
|--------|-------|
| Playwright screenshot + pass/fail | Structured trace with events, messages, graph |
| Guess which subtask failed | See exact verification failure in trace viewer |
| Re-run to debug | Replay saved trace, diff against new run |
| Unit tests only | Trace becomes regression fixture |

## Proposed next steps (in order)

1. **Enrich trace recording in core** — add events to agent.rs paths that
   don't currently emit them (verification, escalation, budget checks)
2. **Trace persistence in cli** — `roco run` and `roco viz` both save traces
   automatically with a UUID
3. **Trace diff CLI** — `roco diff <id1> <id2>` shows what changed
4. **napi-rs scaffold** — `crates/napi/` that exposes `runTask()` to Node.js
5. **Next.js + oRPC app** — minimal app with one procedure that calls the
   addon and returns a trace
