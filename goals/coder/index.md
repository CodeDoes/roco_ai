# Goals: coder

Prerequisite order (top to bottom):

1. **human_approval** — the gate: the agent's proposed actions require human sign-off
2. **devloop** — the agent's own develop → test → lint cycle in a sandbox
3. **sandbox_execution** — isolated execution for untrusted code
4. **testing** — the coder's own test generation and verification
5. **linting** — code style, static analysis, pre-commit checks
6. **package_allowlist** — approved dependency list; blocks supply-chain surprises
