# Architecture

## Core Concepts

### Gateway
Entry point for communication. Handles authentication, routing, and channel dispatch.
- Current: CLI (`cli.ts`)
- Planned: HTTP server, WebSocket, messaging adapters
- Gateways are stateless — they parse input, look up session, forward to channel handler

### Channel
Communication medium. Defines how messages are sent and received.
- Properties: `type`, `messageFormat`, `capabilities` (streaming? file attachment? interactive?)
- Current: `terminal` channel (stdin/stdout)
- Planned: `web` (HTTP/WS), `telegram`, `discord`, `slack`
- Channels register with the gateway, gateway routes messages to the active channel

### Session
Per-user/per-story persistent state container.
- Lives in `s/<story>/` directory
- Contains: message history, state checkpoints, plan, metadata
- Sessions are channel-agnostic — same session works across terminal/web/telegram
- State checkpoints enable instant context restoration (21MB for 2.9B)

### State
The RWKV latent state vector. Fixed-size (~21MB for 2.9B model).
- `_system_baseline.state` — system prompt baked in
- `_state_<name>.state` — named checkpoints (chapters, scenes, decisions)
- States are additive/composable — blend multiple tuned states via weighted sum
- No context window to manage — RWKV's RNN architecture means fixed-size state

## Data Flow

```
User Input
    │
    ▼
Gateway (CLI/HTTP/WS)
    │
    ├── authenticate
    ├── resolve session (by story slug)
    └── route to channel handler
    │
    ▼
Channel Handler (terminal implementation)
    │
    ├── load session (messages + latest checkpoint)
    ├── build prompt (system + history + user input)
    ├── select mode (prose/planning/tool-call based on intent)
    │       │
    │       ▼
    │   State Router (future)
    │   │
    │   ├── detect intent from input
    │   ├── blend mode states (w₁·s₁ + w₂·s₂ + ...)
    │   └── load blended state into engine
    │
    ├── RwkvEngine.generate()
    │       │
    │       ├── load state (baseline + history via evaluateWithoutGeneratingNewTokens)
    │       ├── evaluate (user tokens)
    │       └── generate (async token loop)
    │       │
    │       ▼
    │   Token Stream
    │       │
    │       ├── yield tokens to channel (streaming)
    │       ├── detect tool calls → execute → feed result
    │       └── detect EOS → optional paragraph-break continuation
    │
    ├── save messages to session
    ├── save state checkpoint (optional)
    └── return response to gateway
    │
    ▼
Gateway → User
```

## Layers

```
┌─────────────────────────────────────────┐
│              Gateway Layer               │
│  cli.ts │ http.ts │ telegram.ts │ ws.ts  │
├─────────────────────────────────────────┤
│             Channel Layer                │
│  terminal │ web │ messaging adapters     │
├─────────────────────────────────────────┤
│              Agent Layer                 │
│  storyteller.ts │ coder.ts │ planner.ts  │
├─────────────────────────────────────────┤
│            Session Layer                 │
│  session.ts │ checkpoint manager        │
├─────────────────────────────────────────┤
│             Engine Layer                 │
│  rwkv-engine.ts │ node-llama-cpp        │
├─────────────────────────────────────────┤
│             Model Layer                  │
│  RWKV GGUF │ Vulkan/CUDA │ llama.cpp    │
└─────────────────────────────────────────┘
```

## State Management Strategy

### Why RWKV
Transformers use KV cache proportional to context length. 8K context = ~GB-scale KV cache. RWKV uses a fixed-size recurrent state (~21MB for 2.9B). No context window pressure. True infinite context via state cycling (load baseline → process → save → load next).

### Save Points
- System prompt baseline (always available)
- Chapter/scene boundaries (named checkpoints)
- Before/after tool execution (crash recovery)
- Periodic auto-save (every N steps)

### Memory
Not RAG. Not vector embeddings. Just the state vector. It contains everything the model "remembers". To recall, load the state. To forget, load a different state. States are composable — blend prose state + planning state for structured narrative.
