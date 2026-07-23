# RoCo AI Codebase Analysis — Tests & Evals

## Use Cases by Crate
| Crate | Use Case |
|-------|----------|
| agent-core | Autonomous agent loop (ReAct + Mechanistic), memory, scheduling, reversibility |
| agent-story | Story pipeline: chapter steering, commentary, outline editing, quality, persistence |
| agent | Combined agent + story exports |
| app | AppContext, session agent binding, workspace timeline |
| bnf-engine | Token-level BNF grammar engine (kbnf wrapper) |
| chat-common | Shared chat types across CLI/TUI/web |
| cli | CLI commands (eval, story, desktop, router, server) |
| engine | ModelBackend trait, eval framework, story pipeline evals |
| gateway | HTTP gateway / router |
| grammar | JSON Schema → GBNF, schema builder, output strategies |
| infer-client | Remote inference client |
| inferd | RWKV inference daemon |
| inference | RWKV backend, quantization, sampling |
| message | Chat protocol, formatting, GBNF templates |
| server | HTTP server, routes, config |
| session | Persistent session store, sub-sessions, tracing |
| tools | Tool trait, registry, built-in file/bash/search/write/edit |
| ui | Desktop widgets (chat, markdown editor, pet, session browser) |
| validation | Multi-layer validation (classic, inference, outline, wiki) |
| workspace | Sandbox workspace, file access, version control |

## Existing Tests / Evals
- crates/app/tests/facade.rs
- crates/engine/src/tests/eval_suite.rs
- crates/ui/tests/token0_probe.rs, user_story.rs
- crates/inference/examples/rwkv_test.rs
- crates/cli/src/cmd/eval.rs
- evals/run.sh, evals/results/
- agent-story/src/evals.rs, agent/src/evals.rs, engine/src/eval.rs, engine/src/story_evals.rs
- Added temporary: agent-core/src/tests/mod.rs, agent-story/src/tests/mod.rs, bnf-engine/src/tests/mod.rs, evals/run_missing.sh

## Gaps Found
Crates with NO tests: chat-common, cli, gateway, grammar, message, server, session, tools, ui (only 2), workspace, validation.
No unit tests for validation, session persistence, workspace sandboxing, grammar strategies.
No eval scripts for gateway, session, workspace, validation.

## Filled Gaps (new files)
- crates/session/src/tests/mod.rs
- crates/workspace/src/tests/mod.rs
- crates/grammar/src/tests/mod.rs
- crates/message/src/tests/mod.rs
- crates/validation/src/tests/mod.rs
- crates/chat-common/src/tests/mod.rs
- crates/gateway/src/tests/mod.rs
- crates/tools/src/tests/mod.rs
- evals/session_eval.sh, workspace_eval.sh, validation_eval.sh, grammar_eval.sh
