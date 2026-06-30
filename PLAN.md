# Project Plan

## Architecture

```
Inference Engine (rwkv-engine.ts)
  ⇅ generation, state ops
Agent Engine (agent-engine.ts)
  ⇅ conversation, tool calls, sessions
Gateway (gateway/server.ts)
  ⇅ WS broadcast to all channels
Channels (webapp, TUI, CLI)
```

**Inference Engine** — persistent daemon. Model loads once, runs indefinitely.

**Agent Engine** — conversation logic, tool call handling, session management (labeled context checkpoints).

**Gateway** — thin router between agent engine and channels. Manages WS connections. Broadcasts messages to all connected channels. All channels see the same messages.

**Channel** — UI "window" to the agent. Webapp (browser), TUI (terminal), CLI (scriptable). Multiple channels connected to the same gateway share the same conversation.

**Session** — labeled context checkpoint. Defines what the model can see. UI stores full message history (human reads old messages). Model only gets context from current session forward. Sessions can be saved, loaded, switched by label.

### Key Rules

- Human sees everything (full history in UI)
- Bot only sees current session context
- Sessions are labeled checkpoints you can switch between
- All channels connected to the gateway share the same conversation
- Gateway broadcasts tokens/messages to all channels simultaneously

---

## Phase 1: Foundation ✓

- [x] RWKV engine with Vulkan GPU
- [x] State save/load (baseline + checkpoints)
- [x] System prompt baking into state
- [x] Storytelling agent
- [x] CLI with 7 commands
- [x] LoRA adapter loading
- [x] Paragraph break workaround
- [x] Session persistence

## Phase 2: Agent Loop ✓

- [x] Tool-calling agent loop (`src/agent-loop.ts`)
- [x] Wire existing `tools/` into agent (`src/tool-registry.ts`)
- [x] Fix `find.ts` / `grep.ts` path resolution bugs
- [x] CLI `agent` command
- [x] True token-by-token streaming in `generateStream`
- [x] GBNF grammar for structured output enforcement (via `LlamaGrammarEvaluationState`)
- [x] Session checkpointing in agent loop (turn + pre-tool checkpoints)
- [x] `--depth=N` / `--grammar=PATH` CLI flags
- [x] Tested base RWKV 2.9B agent loop (see findings below)

## Phase 3: Gateway & Channels (Current)

- [x] Agent engine (`src/agent-engine.ts`) — standalone, session management, labeled sessions
- [x] Gateway (`src/gateway/server.ts`) — thin router, WS broadcast to all channels
- [x] Web channel (`webapp/index.html`) — browser dashboard, full history, WS streaming
- [x] TUI (`tui/index.ts`) — terminal UI, direct mode or gateway client
- [x] CLI commands: `gateway` starts engine+server, `tui` interactive mode
- [ ] Multi-channel broadcast (2+ channels seeing same messages)
- [ ] Session persistence (save/restore by label)
- [ ] Session search tool (peek into old session messages)

## Phase 4: Multi-Channel Routing

- [ ] Channel-agnostic session routing
- [ ] File watcher (session changes → broadcast)

## Deferred / Archived

- [ ] Skill system (modular tool directories)
- [ ] Long-term memory (state archiving + retrieval)
- [ ] Cron/scheduled tasks
- [ ] System prompt as routable state
- [ ] Cloud training pipeline

---

## Base RWKV 2.9B Test Findings

Tested `agent` command end-to-end with base RWKV 7 2.9B Q4_K_M on Vulkan.

**What works:**
- Model loads, generates, streams per-token ✓
- With `User: / Assistant:` role markers + few-shot examples, model generates `<tool_call>` tags ✓
- Tool calls parse, execute, feed results back to model ✓
- GBNF grammar constrains output to valid tool call format ✓
- Session checkpointing saves state at each turn ✓

**What needs work (prompt iteration, not code):**
- Model hallucinates `<tool_result>` blocks (generates fake tool output). GBNF grammar prevents this.
- Model sometimes picks wrong tool name (`list` instead of `ls`, or `read` for everything)
- `--fix-paragraphs` causes repetition on long generations
- Without GBNF, model generates natural language + tool calls mixed, but tool call format can be malformed

**Next prompt experiments to try:**
1. GBNF grammar that allows optional natural language before/after tool call
2. Stronger few-shot examples (3-5 instead of 2)
3. Lower temperature (0.5) for more deterministic tool choice
4. Tool names in ALL CAPS in descriptions to stand out (LS, READ, etc.)

## Current Focus

1. Test multi-channel broadcast (webapp + TUI simultaneous)
2. Session persistence by label (save/restore across restarts)
3. Session search tool (load old session context)
4. Polish webapp UI (message history, session management)
