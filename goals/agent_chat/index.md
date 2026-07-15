# Goals: agent_chat

## Grammar-First Principle

Persistent agent sessions maintain conversation state across interactions. Every response is grammar-constrained by BNF grammars, ensuring structural validity and preventing meta-commentary contamination (see `goals/infer/gbnf.md`).

## Prerequisites

Prerequisite order (top to bottom):

1. **folder_bound** — persistent agent session bound to a workspace folder;
   the agent can read/write/run within its designated directory and maintains
   conversation state across sessions via `CompletionRequest::session`
