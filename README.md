# RWKV Agent Harness

Recurrent-neural-network agent harness leveraging RWKV's fixed-size latent state for infinite context, state persistence, and tool-use capabilities.

## Quick Start

```bash
pnpm tell "write a short story"                     # Generate story
pnpm tell --fix-paragraphs "story with paragraphs"  # Fix \n\n EOS issue
pnpm interactive                                    # Interactive mode
pnpm plan "fantasy novel outline"                   # Generate story plan
pnpm chapter --num=1 "write chapter one"            # Chapter with state checkpoint
pnpm continue "continue from last checkpoint"       # Resume from saved state
```

## Features

- **Infinite Context** — RWKV state is fixed-size (~21MB for 2.9B) regardless of context length. No KV cache explosion.
- **State Save/Load** — save conversation state to disk, restore instantly. 21MB per checkpoint.
- **System Prompt Baking** — system prompt processed once, state saved. Every session starts from baked state. No token waste.
- **LoRA Adapters** — load separate LoRA adapters at runtime via `--lora`. No merge needed. ~84MB each.
- **Paragraph Break Fix** — `--fix-paragraphs` workaround for RWKV's `\n\n` → EOS behavior.
- **Vulkan GPU** — runs on Vulkan, CUDA, or CPU. Defaults to Vulkan for cross-GPU compatibility.

## Architecture

```
cli.ts → RwkvEngine → node-llama-cpp → llama.cpp → Vulkan/CUDA
       → SessionManager → s/<story>/_session.json
       → StorytellerAgent → state checkpoints
```

### Key Concepts

- **Gateway** — entry point (CLI, HTTP, WebSocket, messaging channel). Currently CLI-only.
- **Channel** — communication medium (terminal, Telegram, Discord, Web). Extensible.
- **Session** — per-story state container. Messages + state checkpoints + plan.

## Files

| Path | Purpose |
|------|---------|
| `cli.ts` | CLI entry point with 7 commands |
| `src/rwkv-engine.ts` | Core inference engine, state management, LoRA |
| `src/session.ts` | Session persistence |
| `src/storyteller.ts` | Storytelling agent |
| `src/types.ts` | Shared types |
| `s/<story>/` | Per-story session dir |
| `tools/` | File operation tools (agent-ready) |
| `models/` | RWKV GGUF models |

## Options

```
--model=PATH    Model path (default: models/rwkv7-g1g-2.9b-...)
--story=NAME    Story slug (default: "default")
--gpu=TYPE      GPU backend: vulkan | cuda | auto
--lora=PATH     LoRA adapter(s), comma-separated
--fix-paragraphs, -p  Continue past \n\n EOS boundary
```

## Dependencies

- [node-llama-cpp](https://node-llama-cpp.withcat.ai/) — llama.cpp bindings for Node.js
- RWKV-7 2.9B GGUF model (or any RWKV GGUF)
- Vulkan/CUDA-capable GPU
