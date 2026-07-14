# Self-Directed Goals: workspace

Reflection of [`goals/workspace/index.md`](../../goals/workspace/index.md).
This layer was **implemented** (sandbox boundary + workspace-scoped tools).
My self-directed work is integration, coverage, and hardening.

Prerequisite order (mirrors the product layer):

1. **workspace** — ✅ done. `Workspace` with `WorkspaceKind` (eval/temp/user/
   agent/generic), `resolve()` path-escape protection (lexical `..`
   normalization + canonical-prefix check), `cwd`, and `metadata()`. *Self-directed
   hardening:* the canonical-prefix check only triggers for files that already
   exist; add a best-effort canonicalize of the parent chain so a symlink
   created *inside* the root that points outside is still caught.
2. **bash_like_tools** — ✅ done. `WorkspaceBashTool` runs with the workspace
   cwd. *Self-directed:* document clearly that the shell is cwd-scoped, not a
   syscall sandbox; consider a denylist of obviously dangerous commands as a
   belt-and-suspenders measure.
3. **file_tools** — ✅ done. read/write/edit/search/list scoped to the root.

**Integration (my own priority, not a product sub-goal):**
- Wire a `Workspace` into the `agent` CLI example so the default agent run is
  sandboxed by default, not operating on the unrestricted global tools.
- Add an **eval case** that asserts the sandbox rejects escape attempts and
  allows in-bounds traversal (so a regression in `resolve()` fails CI).
- Provide ready-made workspace presets: `eval`, `temp`, `user`, `agent`
  constructors that pick sensible roots (temp dir, `./.roco/workspace`, etc.).

**Next self-directed action:** add the sandbox eval case + agent-example
integration so the workspace layer is exercised end-to-end, not just unit-tested.
