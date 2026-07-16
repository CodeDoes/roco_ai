# Goals: workspace

## Grammar-First Principle

The workspace provides the bounded execution environment for grammar-constrained agent actions. Every tool call and file operation happens within a sandbox where output is structurally guaranteed by BNF grammars (see `goals/infer/gbnf.md`).

## Prerequisites

Prerequisite order (top to bottom):

1. **workspace** — the workspace model (what the workspace is, boundaries, metadata)
2. **bash_like_tools** — shell-like tools the agent can run (ls, cat, grep, etc.)
3. **file_tools** — file read/write/search within the workspace boundary
4. **runtime_directory** — canonical `.roco/` runtime layout (sessions, workspaces, tests)


## Status & Self-Directed Actions

My self-directed work is integration, coverage, and hardening.

Prerequisite order (mirrors the product layer):

1. **workspace** ✅ done. `Workspace` with `WorkspaceKind` (eval/temp/user/
   agent/generic), `resolve()` path-escape protection (lexical `..`
   normalization + canonical-prefix check), `cwd`, and `metadata()`. **Symlink
   hardening done:** the canonical-prefix check now catches symlinks created
   *inside* the root that point outside (covered by a regression test).
2. **bash_like_tools** ✅ done. `WorkspaceBashTool` runs with the workspace
   cwd. *Self-directed:* document clearly that the shell is cwd-scoped, not a
   syscall sandbox; consider a denylist of obviously dangerous commands as a
   belt-and-suspenders measure.
3. **file_tools** ✅ done. read/write/edit/search/list scoped to the root.

**Integration (my own priority, not a product sub-goal):**
- ✅ Wire a `Workspace` into the `agent` CLI example so the default agent run is
  sandboxed by default (committed earlier this session).
- ✅ **Sandbox-escape eval case** added: `crates/workspace/src/workspace.rs`
  `mod tests` now has a dedicated regression guard — `escape_via_parent_traversal_is_blocked`,
  `escape_via_absolute_path_is_blocked`, `read_tool_blocks_traversal_escape`,
  `escape_via_symlink_is_blocked` (unix), and `legit_in_bounds_access_still_works`
  — that plants a secret outside the root and asserts neither lexical traversal
  nor symlink escape can reach it, through both `resolve()` and the `read` tool.
- ✅ **Workspace presets/constructors**: `Workspace::preset(kind)` and
  `Workspace::preset_in(kind, base)` pick conventional roots — `Agent` →
  `.roco/workspace/agent` (persistent), `User` → the base dir, and
  `Eval`/`Temp`/`Generic` → an isolated temp dir. Unit-tested.
- ✅ **Bash denylist**: `blocked_command_reason` refuses a small, conservative
  set of destructive/escape-prone command patterns (e.g. `rm -rf /`, `mkfs`,
  fork-bomb); `WorkspaceBashTool` enforces it and is unit-tested.

**Next self-directed action:** move to the `message` layer's remaining items
 (`state_tune_examples`, `system_instruction_following`, `user_message_response`).
