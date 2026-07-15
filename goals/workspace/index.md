# Goals: workspace

## Grammar-First Principle

The workspace provides the bounded execution environment for grammar-constrained agent actions. Every tool call and file operation happens within a sandbox where output is structurally guaranteed by BNF grammars (see `goals/infer/gbnf.md`).

## Prerequisites

Prerequisite order (top to bottom):

1. **workspace** — the workspace model (what the workspace is, boundaries, metadata)
2. **bash_like_tools** — shell-like tools the agent can run (ls, cat, grep, etc.)
3. **file_tools** — file read/write/search within the workspace boundary
4. **runtime_directory** — canonical `.roco/` runtime layout (sessions, workspaces, tests)
