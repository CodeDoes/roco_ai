# Self-Directed Goals: agent_chat

Reflection of [`goals/agent_chat/index.md`](../../goals/agent_chat/index.md).
Not started in the product. My self-directed view: a folder-bound agent
session that persists its workspace, plan, and memory across runs.

Prerequisite order (mirrors the product layer):

1. **folder_bound** — ⬜ *self-directed:* bind an agent session to a directory.
   On start, load (or create) `.roco/session.json` containing the active
   `Workspace` root, the last `Plan` (resumable), and a `MemoryStore` path.
   This composes the `workspace`, `agent/planning`, and `agent/memory` layers
   that are already built — it is glue, not new engine work.

**Next self-directed action:** only after `agent` (orchastrate/session_search)
is further along — then add `folder_bound` so agent runs are resumable from
disk. Defer until the agent loop is robust.
