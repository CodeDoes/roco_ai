# Goals: agent_chat

## Grammar-First Principle

Persistent agent sessions maintain conversation state across interactions. Every response is grammar-constrained by BNF grammars, ensuring structural validity and preventing meta-commentary contamination (see `goals/infer/gbnf.md`).

## Prerequisites

Prerequisite order (top to bottom):

1. **folder_bound** — persistent agent session bound to a workspace folder;
   the agent can read/write/run within its designated directory and maintains
   conversation state across sessions via `CompletionRequest::session`


## Status & Self-Directed Actions

survive across invocations. This layer makes that real: a *folder-bound*
agent session that persists a workspace + memory + session history (and, via
the recorded `AgentTrace`, the plan that was executed) so the agent continues
where it left off.

Prerequisite order (mirrors the product layer):

1. **folder_session** ✅ done. `AgentChatSession`
   (`crates/agent/src/agent_chat.rs`) opens (or initializes) a session rooted
   at a project `<folder>`: it loads `MemoryStore` + `SessionStore` from
   `<folder>/.roco/agent_chat/`, roots the agent's `Workspace` at the folder
   (so it can read/edit the project), and runs tasks with the combined
   built-in + workspace + memory + session + scheduler tools. Both stores
   persist on every write, so reopening the same folder restores continuity.
   Unit-tested: memory + session history survive a reopen, and the combined
   tool set includes the workspace/persistent tools.
2. **resume_plan** 🟡 *self-directed:* the executed plan is captured in the
   recorded `AgentTrace` (and thus the `SessionStore` transcript), so a resumed
   session can `search_sessions` for prior plans. A future step could lift a
   prior `Plan`/`PlanStep` list back into the agent's working set explicitly;
   for now the transcripts are the source of truth and are searchable.

**Wiring (my own priority):** the `agent_chat` CLI example
(`crates/cli/examples/agent_chat.rs`) opens a folder, runs a task against the
RWKV backend, runs due scheduled tasks, and persists — so the feature is
reachable end-to-end, not just compiled into the crate.

**Next self-directed action:** let a resumed session actively *reuse* a prior
plan (lift the last `SessionTranscript`'s plan steps back into the agent), then
move to `browser_use` (deferred until the agent loop is robust) or the `coder`
capstone.
