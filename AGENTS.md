# For AI Coding Agents

Active development. Storyteller agent + tool-calling agent loop + gateway/channels. See `PLAN.md` for milestones. Phase 3 (Gateway & Channels) current.

## Commands

| `pnpm ...` | What |
|------------|------|
| `tell "prompt"` | Generate story text |
| `agent "prompt"` | Agent mode with tool use (`--depth=N`, default 5) |
| `chapter --num=N "prompt"` | Write chapter, save checkpoint |
| `plan "outline"` | Generate story plan, save to `sessions/<session>/_plan.md` |
| `interactive` | REPL mode (`exit` to quit, `save` to checkpoint) |
| `continue "prompt"` | Resume from latest checkpoint |
| `checkpoint save\|load\|ls [name]` | Manual checkpoint ops |
| `state-info` | Show engine/session state |
| `gateway` | Start engine + HTTP/WS server (default port 3030) |
| `tui [--connect]` | Terminal UI (direct engine or gateway client) |
| `typecheck` | `tsc --noEmit` |

Default model: `models/rwkv7-g1g-2.9b-20260526-ctx8192-Q4_K_M.gguf`

## File Layout

| Path | Role |
|------|------|
| `cli.ts` | Entry point, 10 commands, arg parsing |
| `src/core/types.ts` | Shared type definitions |
| `src/core/session.ts` | Session persistence, JSONL event log (`sessions/<id>/session.jsonl`) |
| `src/core/agent-loop.ts` | Tool-call loop runtime: generate → parse → execute → feedback. Uses `stopSequences: ["</tool_call>"]` to cut generation at tool call boundary. Accepts custom `AgentLoopConfig` (systemPrompt, toolDefs, toolHandlers). |
| `src/core/agent-engine.ts` | Standalone agent engine with labeled session management |
| `src/core/tool-registry.ts` | Global tool defs + handlers, XML serialization. `toolsToXml(defs?)` accepts optional custom tool set. |
| `src/engine/rwkv-engine.ts` | llama.cpp backend via `node-llama-cpp`. Model lifecycle, state save/load, generate, LoRA, MoSE. |
| `src/engine/mose-engine.ts` | MoSE (state blending) + LoRAManager (adapter switching). Used by RwkvEngine. |
| `src/agents/envoy/` | User-facing agent. Only `spawn_agent` tool. Delegates to subagents (storyteller, coder). |
| `src/agents/storyteller/` | Story generation agent (prose mode) — 5 file tools + story-analyze/validate |
| `src/agents/storyteller/skills/` | Skill modules + `tools/story-analyze.ts`, `tools/story-validate.ts` |
| `src/agents/coder/` | Code agent (skeleton) — full 7-tool access |
| `src/tools/*.ts` | Shared tool implementations (read, write, edit, ls, mkdir, grep, find) |
| `src/gateway/server.ts` | Express + WebSocket server, REST chat + WS broadcast |
| `src/channels/tui/index.ts` | Terminal UI (direct engine or gateway client) |
| `src/channels/web/index.html` | Browser dashboard (served by gateway) |
| `src/grammars/tool_call.gbnf` | GBNF grammar constraining tool call output |
| `src/eval/mock-engine.ts` | Reusable `MockEngine` class for deterministic eval |
| `src/eval/trace-writer.ts` | Sync file writer for eval traces (`src/eval/.traces/`, gitignored) |
| `src/eval/story-creation.eval.ts` | Envoy→storyteller eval. Oracle mode (mock) + live mode (real model) |
| `src/grammars/tool_call.gbnf` | GBNF grammar constraining tool call output |
| `docs/agent-behavior/` | Behavioral spec docs — universal-base, envoy, storyteller, coder |
| `docs/synthdata/` | Training dataset examples — multi-turn conversational patterns |
| `sessions/<ts>_<id>_<slug>/` | Per-session dir: `session.jsonl`, `_state_*.state`, `_system_baseline.state` |
| `workspace/` | Project files |
| `docs/` | Architecture docs + future plans |

## Engine

Single backend — `RwkvEngine` implements `Engine` interface via `node-llama-cpp`:

```
RwkvEngine ──▸ MoSEEngine (state blending)
        └──▸ LoRAManager (LoRA adapter switching)
```

`cli.ts:createEngine()` returns `Engine` interface — agents communicate through it.

## Architecture

3 agent layers: Envoy → Subagent (storyteller/coder) → Session → Engine

```
User → EnvoyAgent (spawn_agent only)
         → storyteller sub-agent (AgentLoop with storyteller tools + instructions)
         → coder sub-agent (AgentLoop with coder tools + instructions)
              → SessionManager → sessions/<id>/
              → RwkvEngine (node-llama-cpp)
```

Envoy delegates to specialized subagents via `spawn_agent`. Subagents run their own `AgentLoop` instance with dedicated tools and instructions. Results flow back to the envoy for user presentation.

Gateway mode (`cli.ts gateway`) wraps the engine in an Express server for remote access.

Key pattern — constructor injection + AgentLoopConfig:
```ts
const engine = new RwkvEngine(modelPath, sessionDir)
const session = new SessionManager(sessionsDir, story, modelPath)
const agent = new StorytellerAgent(engine, session, config)
const agentLoop = new AgentLoop(engine, session, maxDepth, {
  systemPrompt: "...",          // custom system prompt
  toolDefs: storytellerToolDefs, // custom tool definitions
  toolHandlers: {...},          // custom tool handlers
})
const gateway = new GatewayServer(agentEngine, webappDir)
```

## State Management

- RWKV state is fixed-size (~21MB for 2.9B). No KV cache growth.
- System prompt baked once via `bakeSystemPrompt()` → `_system_baseline.state`
- Named checkpoints: `_state_<name>.state`
- State checkpoints only valuable after significant token ingestion or to lock behavior modes. Saving after short exchanges wastes I/O for ~21MB per write.
- Always save before destructive ops, restore on failure
- Agent loop does NOT save per-turn checkpoints (wasteful I/O for short exchanges). Checkpoints only at chapter boundaries or manual save.
- Restore strategy: load closest previous checkpoint, replay remaining messages to rebuild state.

## Tool Protocol

Model outputs `<tool_call>\n{"name": "...", "args": {...}}\n</tool_call>`. Agent feeds back `<tool_result name="..." success="true|false">\n...\n</tool_result>`. Results truncated to 2000 chars.

GBNF grammar at `src/grammars/tool_call.gbnf` constrains output to valid tool calls. Load via `--grammar=tool_call.gbnf` (resolved from `src/grammars/` if relative).

**Stop sequence**: Harness uses `stopSequences: ["</tool_call>"]` to terminate generation the instant the closing tag appears. This prevents the model from hallucinating `<tool_result>` blocks — harness injects the real result instead.

## Known Quirks

- `generateStream` is a batch wrapper, not true streaming. `generate()` builds full result then feeds to callback.
- `fixParagraphBreak` heuristic: detects EOS after `\n\n`, injects newline token to continue. Quality degrades over repeated fixes.
- LoRA API via `any` cast on `node-llama-cpp` types.
- `<think>` blocks preserved in session history and fed back as context for state consistency. RNN state encodes every token — removing blocks breaks what model "remembers". Still stripped from final return/display output only.
- Model sometimes hallucinates `<tool_result>` blocks (GBNF grammar prevents); pick wrong tool names (needs prompt tuning).

## Testing

Only `pnpm typecheck` (`tsc --noEmit`). No test runner.

### Agent Oracle Eval

`src/eval/story-creation.eval.ts` — two modes:

| `pnpm eval` | Oracle mode (default). MockEngine with scripted tool calls simulating envoy → storyteller chain. Verifies exact file creation (plan + 3 chapters + 3 wiki entries). No model needed. Fast (~1s). |
|-------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `pnpm eval:live` | Live mode. Runs against real model with same workflow. Checks structural match (does agent create plan + chapters + wiki?). Keeps files in `/tmp/eval-story-*` for inspection. |

Oracle demonstrates correct workflow sequence. If live mode fails, tune system prompt examples in `buildSystemPrompt()` at `src/eval/story-creation.eval.ts`.

`src/eval/mock-engine.ts` — reusable `MockEngine` class for other evals. `src/eval/trace-writer.ts` saves structured traces to `src/eval/.traces/` (gitignored) with streamed model output, input markers, tool calls, and verification results.

## Config

- `pnpm` required (v11.9.0). Lockfile: `pnpm-lock.yaml`. Workspace: `pnpm-workspace.yaml` (allows `esbuild` and `node-llama-cpp` builds).
- ESM (`"type": "module"`). Run with `tsx`. TypeScript 6.0.
- GPU defaults to Vulkan. Override `--gpu=cuda` or `--gpu=auto`.
- LoRA via `--lora=path1.gguf,path2.gguf` (relative paths resolved from project root).
- `node-llama-cpp` v3.18.1, Linux Vulkan bindings.
