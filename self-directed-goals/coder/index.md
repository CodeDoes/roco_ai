# Self-Directed Goals: coder

Reflection of [`goals/coder/index.md`](../../goals/coder/index.md). The
capstone — the agent running its own develop/test/lint loop. Not started;
depends on `workspace`, `agent` orchestration, and a human-approval gate.

Prerequisite order (mirrors the product layer):

1. **sandbox_execution** — ⬜ *self-directed:* reuse `Workspace` + the
   workspace-scoped `bash` tool as the execution sandbox. Already largely built.
2. **package_allowlist** — ⬜ *self-directed:* restrict which crates/commands
   the coder may touch, derived from the workspace root and a denylist.
3. **testing** — ⬜ *self-directed:* the coder runs `cargo test` on the touched
   crate and reads results as a tool outcome.
4. **linting** — ⬜ *self-directed:* run `cargo clippy` / `cargo fmt --check`
   and treat findings as feedback.
5. **human_approval** — ⬜ *self-directed:* before applying non-test edits or
   running outside the sandbox, request approval (a `request_approval` tool
   that blocks until acknowledged). This is the safety gate that makes the
   coder safe.
6. **devloop** — ⬜ *self-directed:* the observe→edit→test→lint→repeat loop,
   driven by `Plan::execute` with `bash`/`read`/`write`/`edit` tools.

**Next self-directed action:** the furthest-out layer. Only pursue after
`agent` (orchastrate) and `workspace` integration are solid. This is where the
agent becomes self-improving — the ultimate expression of the `self_improvement`
meta-goal.
