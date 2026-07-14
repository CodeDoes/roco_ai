# Agent Chat

Intent: A persistent agent session bound to a workspace — either a permanent
workspace (the agent's "home") or a folder the user designates as its
working context. The agent can read, write, and run commands within that
workspace, maintaining conversation history and state across sessions.

## What it is

A `roco agent` subcommand that runs an autonomous agent loop inside a
workspace directory. The agent:

1. **Receives a task** (from the user or a queue)
2. **Reads the workspace** (files, git status, project structure)
3. **Plans and acts** (writes files, runs commands, checks results)
4. **Reports back** (success, failure, or asks for clarification)
5. **Persists state** — conversation history, tool call log, and RWKV
   session state are saved so the agent can resume later

### Workspace modes

| Mode | Description | Use case |
|---|---|---|
| **Permanent** | Agent has a fixed home directory (e.g. `~/.roco/workspace`) that persists across all sessions. | Personal assistant, ongoing projects |
| **Folder-bound** | User points the agent at a specific folder (`roco agent ./my-project`). The agent's scope is limited to that tree. | Project-specific tasks, code review, debugging |
| **Ephemeral** | Temporary workspace, wiped after the session. | One-off tasks, sandboxed experiments |

## Requirements

### Core agent loop
- **Task intake** — user gives a natural-language task
- **Context gathering** — read workspace files, git status, project structure
- **Plan → Act → Observe** loop with a step budget
- **Tool use** — file read/write, shell commands, grep/search
- **Self-correction** — if a command fails, the agent retries with a fix

### Workspace integration
- **File access** — read/write within the workspace boundary
- **Command execution** — run shell commands, capture output
- **Git awareness** — detect repo root, show status, respect `.gitignore`
- **Project detection** — identify language/framework from file patterns

### Session persistence
- **Conversation history** — saved between sessions (JSON transcript)
- **RWKV state** — uses `CompletionRequest::session` to carry model state
- **Tool call log** — what the agent did, for audit and replay
- **Resume** — `roco agent --resume` picks up where it left off

### Safety
- **Workspace boundary** — agent can only access files within its workspace
- **Command allowlist** — only approved commands (no `rm -rf /`, no network exfil)
- **Human approval gate** — risky actions (deleting files, running unknown binaries) require confirmation
- **Step budget** — max N steps per session to prevent runaway loops

## Dependencies

| Dep | Goal | Status |
|---|---|---|
| `infer/inference` | RWKV inference engine | ✅ Done |
| `infer/state_mixing` | Session state pool | ✅ Phase 1 done |
| `message/system_instruction` | System prompts | ✅ Done |
| `message/tool_calling` | Tool call format | In progress |
| `message/tool_catelogue` | Tool schema definitions | ✅ Done |
| `message/chat_cli` | Chat REPL | ✅ Done |
| `workspace/` | Workspace model | In progress |
| `testing/eval_harness` | Regression gates | Phase 1 planned |

## Implementation plan

### Phase 1: Folder-bound agent
- `roco agent ./path` — single-session agent in a specific folder
- Basic tool set: read file, write file, run command, grep
- Simple agent loop: plan → one tool call → observe → repeat
- No persistence yet; conversation and state lost on exit

### Phase 2: Session persistence
- Save conversation transcript to `.roco/agent/transcript.json`
- Use `CompletionRequest::session` to persist RWKV state
- `roco agent --resume ./path` — picks up the last session

### Phase 3: Permanent workspace
- `roco agent` (no path) uses a permanent workspace directory
- Agent maintains long-term context across tasks
- Workspace metadata: project type, last activity, open tasks

### Phase 4: Safety and gates
- Human approval gate for destructive actions
- Step budget with configurable limit
- Command allowlist / blocklist
- Audit log of all agent actions

## Where it lives

- `crates/core/examples/agent_chat.rs` — initial implementation
- Eventually: `crates/agent/` crate with the agent loop, workspace model,
  and safety gates
- The `roco agent` CLI entry point via devenv.nix

## Alternatives considered

- **Just extend the chat CLI** — the chat CLI is for conversation; the agent
  needs an autonomous loop with tool use, workspace awareness, and safety
  gates. Different surface.
- **Use a framework** (CrewAI, LangGraph, AutoGen) — defeats the purpose of
  the local RWKV engine. The whole point is running everything on-device.
- **Web UI** — eventually yes, but the CLI comes first. Terminal is the
  fastest path to a working agent; the web UI is a layer on top.
