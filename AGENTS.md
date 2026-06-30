# For AI Coding Agents

## Project State

Active development. Working storyteller agent with RWKV state management. Tools exist but not wired into agent loop yet. See PLAN.md for current milestones.

## Important Files

| File | Must-Read For |
|------|---------------|
| `cli.ts` | Entry point, argument parsing |
| `src/rwkv-engine.ts` | Core engine: model lifecycle, state save/load, generate, LoRA |
| `src/storyteller.ts` | Story agent: prompt building, chapter checkpoints, output cleaning |
| `src/agent-loop.ts` | Agent loop: tool-call parsing, execution, multi-turn feedback |
| `src/tool-registry.ts` | Tool definitions, descriptions, XML serialization for system prompt |
| `src/session.ts` | Session JSON persistence |
| `src/types.ts` | All shared type definitions (including ToolCall, ToolResult, ToolDef) |
| `tools/*.ts` | File operation modules (now wired via agent-loop) |
| `docs/*.md` | Architecture decisions and future plans |
| `docs/future/*.md` | Aspirations, not implemented |

## Key Patterns to Follow

### Engine + Session + Agent Composition
Engine handles model I/O. Session handles persistence. Agent handles orchestration. They compose via constructor injection:
```ts
const engine = new RwkvEngine(modelPath, stateDir)
const session = new SessionManager(stateDir, story, modelPath)
const agent = new StorytellerAgent(engine, session, config)
```

### State Checkpoints
Always save state before potentially destructive operations. Restore on failure. Checkpoints are full sequence states (~21MB for 2.9B).

### Agent Loop
`AgentLoop` wraps engine + session, runs `generate → parseToolCalls → execute → feedback → repeat`. Uses `<tool_call>` / `<tool_result>` XML tags. Call `agent.run(userInput)` for single request. Text output is accumulated across all turns; tool calls and results are stripped from visible output but kept in prompt context.

Tools are defined in `tool-registry.ts` with `ToolDef` interface (name, description, parameters). Tool handlers live alongside tool definitions and delegate to `tools/*.ts` functions.

### Tool Call XML Format
Model outputs:
```xml
<tool_call>
{"name": "read", "args": {"path": "file.txt"}}
</tool_call>
```
Agent feeds back:
```xml
<tool_result name="read" success="true">
file content here
</tool_result>
```

### LoRA Through `any` Cast`

### Module Pattern
Tools export default functions. Functions are pure (input → output). No side effects except filesystem. See `tools/` for examples.

## Known Quirks

- `generateStream` is currently a batch wrapper, not true streaming. `generate()` builds full result, then feeds to callback.
- `fixParagraphBreak` is a heuristic — detects EOS after `\n\n`, injects newline token to continue. Quality degrades over repeated fixes.
- EOS token access via `(model as any).tokenEos?.()` — fragile cast.
- `find.ts` and `grep.ts` have path resolution bugs (don't join parent dir on recursive calls).

## Testing

```bash
pnpm typecheck  # tsc --noEmit
# No test runner configured yet
```

## LoRA Training Pipeline

1. Get `.pth` base model from HuggingFace
2. Prepare jsonl dataset
3. Train via RWKV-PEFT on Runpod/Kaggle
4. Convert `.pth` LoRA → `.gguf` LoRA
5. Load with `--lora=adapters/story.gguf`
