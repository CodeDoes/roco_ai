# Chat CLI

Intent: A simple terminal-based chat interface for the local RWKV model,
supporting multi-turn conversations with session persistence, grammar
constraints, and streaming output.

## What it is

A `roco chat` subcommand that provides a REPL for chatting with the model:

```
$ roco chat
> What's the capital of France?
Paris.
> What's its population?
About 2.2 million in the city proper, 13 million in the metro area.
> /help
Commands: /help /quit /save <name> /load <name> /grammar <file> /temp <n>
> /quit
```

## Requirements

### Core
- **REPL loop** — reads user input, sends to backend, streams response back
- **Session persistence** — uses `CompletionRequest::session` to maintain
  conversation state across turns (the Phase 1 state pool in
  `rwkv_backend.rs`)
- **Streaming** — token-by-token output via `on_token` callback
- **Interrupt** — Ctrl+C cancels the current generation (uses
  `ModelBackend::interrupt`)

### Session management
- `/save <name>` — names the current session so it persists after exit
- `/load <name>` — loads a named session (creates if new)
- `/clear` — resets to blank state, starts fresh conversation
- Sessions stored in the actor's `state_pool` HashMap during the process
  lifetime; optional disk serialization for persistence across restarts

### Grammar constraints
- `/grammar <file>` — load a GBNF grammar file for constrained generation
- `/grammar off` — disable grammar, back to free-form
- Uses the existing `CompletionRequest::grammar` + `RWKV_GRAMMAR` plumbing

### Configuration (runtime commands)
- `/temp <n>` — set sampling temperature (0.0 = greedy, 1.0 = creative)
- `/max <n>` — set max tokens per response
- `/system <text>` — set the system/instruction prompt
- `/stats` — show tokens/sec, total tokens, session info

## Dependencies (prerequisite goals)

All prerequisites are now **done**:

| Dep | Goal | Status |
|---|---|---|
| `infer/inference` | RWKV inference engine | ✅ Done |
| `infer/streaming` | Token streaming via `on_token` | ✅ Done |
| `infer/state_mixing` | Session state pool (Phase 1) | ✅ Done |
| `infer/interrupt_inference` | Generation interrupt (Ctrl+C) | ✅ Done |
| `infer/continue_inference` | Continue inference (preserve state) | ✅ Done |
| `infer/gbnf` | Grammar-constrained decoding | ✅ Done |
| `message/system_instruction_following` | System instruction following | ✅ Done |
| `message/user_message_response` | User message response formatting | ✅ Done |

## Implementation plan

### Phase 1: Basic REPL
- Add `roco chat` subcommand (via devenv.nix `scripts.*.exec`)
- Simple readline loop: read → `backend.complete()` → print response
- Use `session: Some("default")` for multi-turn state persistence
- Ctrl+C → `backend.interrupt()`

### Phase 2: Session management
- Named sessions with `/save`, `/load`, `/clear`
- Session metadata (created, last-used, token count)
- LRU eviction with configurable `max_sessions`

### Phase 3: Polish
- Syntax-highlighted streaming output (crossterm / ratatui)
- Command history (rustyline or reedline)
- Grammar toggle, temperature slider
- `/stats` output (tokens/sec, session info)

### Phase 4: Disk persistence (optional)
- Serialize `state_pool` to disk on exit
- Deserialize on startup
- Uses `TensorCpu<f32>` serialization from `web_rwkv::tensor::serialization`

## Where it lives

- `crates/cli/examples/chat.rs` — initial implementation
- Eventually: a dedicated `crates/cli/` crate if the CLI grows beyond
  the devenv script wrapper

## Hardware notes

- The chat CLI uses the same inference path as `rwkv_test` / `eval_suite`
- Session state (~1.4 GB for 2.9B NF4) stays in GPU VRAM between turns
- Multiple named sessions swap in/out of the single GPU slot via LRU
- No additional VRAM beyond what the inference engine already uses

## Alternatives considered

- **Just use the existing eval examples** — they're batch-oriented, not
  interactive. A chat CLI needs streaming output, interrupt handling, and
  a proper REPL loop.
- **Use a TUI framework from day one** — overkill for v1. Start with a
  simple readline loop; add crossterm/ratatui in Phase 3 if it feels
  cramped.
- **Integrate with an existing chat tool** (e.g. LLM CLI, Ollama) — defeats
  the purpose of having a local RWKV engine. The whole point is to test
  our own inference stack end-to-end.
