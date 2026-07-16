# Goals: coder

## Grammar-First Principle

Code generation and manipulation are grammar-constrained by BNF grammars. The devloop, sandbox execution, and linting all operate on structurally guaranteed outputs (see `goals/infer/gbnf.md`). Human approval gates remain the final safety check.

## Prerequisites

Prerequisite order (top to bottom):

1. **human_approval** — the gate: the agent's proposed actions require human sign-off
2. **devloop** — the agent's own develop → test → lint cycle in a sandbox
3. **sandbox_execution** — isolated execution for untrusted code
4. **testing** — the coder's own test generation and verification
5. **linting** — code style, static analysis, pre-commit checks
6. **package_allowlist** — approved dependency list; blocks supply-chain surprises


## Status & Self-Directed Actions

depends on `workspace`, `agent` orchestration, and a human-approval gate.

Prerequisite order (mirrors the product layer):

1. **sandbox_execution** ⬜ *self-directed:* reuse `Workspace` + the
   workspace-scoped `bash` tool as the execution sandbox. Already largely built.
2. **package_allowlist** ⬜ *self-directed:* restrict which crates/commands
   the coder may touch, derived from the workspace root and a denylist.
3. **testing** ⬜ *self-directed:* the coder runs `cargo test` on the touched
   crate and reads results as a tool outcome.
4. **linting** ⬜ *self-directed:* run `cargo clippy` / `cargo fmt --check`
   and treat findings as feedback.
5. **human_approval** ⬜ *self-directed:* before applying non-test edits or
   running outside the sandbox, request approval (a `request_approval` tool
   that blocks until acknowledged). This is the safety gate that makes the
   coder safe.
6. **devloop** ⬜ *self-directed:* the observe→edit→test→lint→repeat loop,
   driven by `Plan::execute` with `bash`/`read`/`write`/`edit` tools.

**Next self-directed action:** the furthest-out layer. Only pursue after
`agent` (orchastrate) and `workspace` integration are solid. This is where the
agent becomes self-improving — the ultimate expression of the `self_improvement`
meta-goal.
