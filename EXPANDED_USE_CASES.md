# Expanded Local Agent Use Cases — Implementation Plan

## Domains & Crate Mapping
| Domain | Existing Crate / Module | Initial Implementation Target |
|--------|-------------------------|-------------------------------|
| Writing / Story | agent-story, agent, validation | Story pipeline + critique |
| Coding | agent-core (mecha_agent), cli (coder) | Code-driven agent loop for file edits |
| HTML | cli (html), workspace (write/edit) | HTML generation + preview workspace |
| Chat | chat-common, agent-core (agent_chat), ui (chat) | Multi-turn session persistence |
| Organization | session, workspace, tools | Workspace timeline + session pool |
| Desktop Pet | ui (pet) | Always-on-top widget using local inference |
| Debugging | tools (builtins), validation (inference) | Bash/read/error analysis pipeline |
| Emails | message, grammar | Structured message/GBNF templates |
| Research | agent-core (memory, sessions), infer-client | Recall + remote fallback option |
| Aggregating | engine (eval), workspace (version) | Snapshot comparison + eval aggregation |
| Browser Use | workspace, gateway, server | Local server gateway for web surface |

## Gaps to Fill First (from earlier analysis)
- session/src/tests (persistence)
- workspace/src/tests (sandbox)
- validation/src/tests (critique pipeline)
- grammar/src/tests (structured output)
- message/src/tests (protocol)
- tools/src/tests (builtins)
- gateway/src/tests (routing)

## Next Step
Choose 1 domain for first implementation file, or start with a unified `local_agent` scaffold in `crates/app` that binds all domains.
