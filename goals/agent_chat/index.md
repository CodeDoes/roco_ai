# Goals: agent_chat

Prerequisite order (top to bottom):

1. **folder_bound** — persistent agent session bound to a workspace folder;
   the agent can read/write/run within its designated directory and maintains
   conversation state across sessions via `CompletionRequest::session`
