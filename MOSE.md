# Mixture of State Experts (MoSE) + Mixture of LoRA Experts (MoLE)

RWKV's fixed-size recurrent state enables something impossible with transformers: **state blending**. Blend N expert states at the binary level (weighted float32 sum) → single forward pass with combined behavior.

## Architecture

MoSE + MoLE live in `RwkvEngine` (node-llama-cpp backend). The gateway (`cli.ts gateway`) wraps `RwkvEngine` and exposes MoSE/MoLE via REST endpoints.

```
cli.ts ──▸ RwkvEngine ──▸ MoSEEngine (state blending)
                    └─▸ LoRAManager (LoRA adapter switching)
```

Gateway (port 3030) exposes MoSE/MoLE API for remote clients.

## How State Blending Works

1. `createExpert(name, text)` — evaluates text through model, saves resulting state as binary `.state` file
2. `blend(weights)` — reads N expert state files, element-wise weighted float32 sum, normalizes
3. `apply(sequence)` — loads blended state into active sequence

State files are opaque binary blobs from llama.cpp. For RWKV models these are recurrent state tensors (float32), so element-wise blending is safe.

## MoSE CLI Usage

```bash
# Create expert (state baked from text)
pnpm tsx cli.ts mose expert create formal \
  --text="You write with academic formality. Precise vocabulary, structured paragraphs."

pnpm tsx cli.ts mose expert create creative \
  --text="You write with vivid imagery. Metaphors, sensory detail, varied sentence rhythm."

# List experts
pnpm tsx cli.ts mose expert ls

# Blend and generate
pnpm tsx cli.ts mose generate "Explain quantum computing" formal=0.7 creative=0.3

# Standard generate (no blend)
pnpm tsx cli.ts tell "Write a story about AI"

# Gateway mode with MoSE API
pnpm tsx cli.ts gateway
```

## MoLE CLI Usage

```bash
# Register LoRA adapters
pnpm tsx cli.ts lora add formal --file=loras/formal.gguf
pnpm tsx cli.ts lora add creative --file=loras/creative.gguf

# List
pnpm tsx cli.ts lora ls

# Activate (hot-swap without reloading model)
pnpm tsx cli.ts lora activate formal

# Deactivate
pnpm tsx cli.ts lora deactivate
```

## Gateway API

When running `pnpm tsx cli.ts gateway`, MoSE/MoLE exposed at:

| Method | Path | Body | Description |
|--------|------|------|-------------|
| POST | `/mose/experts` | `{name, text, weight?}` | Create expert from text (eval + save state) |
| GET | `/mose/experts` | | List experts |
| DELETE | `/mose/experts/:name` | | Remove expert |
| POST | `/mose/blend` | `{weights: {name: w, ...}}` | Blend experts into sequence |
| POST | `/mose/generate` | `{prompt, blend?, ...genOpts}` | Blend then generate |
| POST | `/mose/segment` | `{segments: [{text, blend}]}` | Segment routing |
| POST | `/lora/experts` | `{name, filePath, scale?}` | Register LoRA adapter |
| GET | `/lora/experts` | | List + active adapters |
| DELETE | `/lora/experts/:name` | | Remove adapter |
| POST | `/lora/activate` | `{adapters: [name,...]}` | Activate adapter(s) |
| POST | `/lora/deactivate` | | Deactivate all |

## Segment Routing

Process prompt segments with different expert blends:

```json
POST /mose/segment
{
  "segments": [
    {"text": "System: You are a writing assistant.", "blend": {"formal": 1.0}},
    {"text": "User: Write a poem about autumn.", "blend": {"creative": 0.8, "precise": 0.2}}
  ]
}
```

Each segment evaluates with its blend, accumulating state for the next segment. Last segment's text is used for generation.

## Implementation

### Files

| File | Role |
|------|------|
| `src/engine/mose-engine.ts` | `MoSEEngine` (state blend) + `LoRAManager` (LoRA switching) |
| `src/engine/rwkv-engine.ts` | `RwkvEngine implements Engine` — inference backend with MoSE/MoLE |
| `src/core/types.ts` | `Engine`, `MoSEHandle`, `LoRAHandle` interfaces |
| `src/gateway/server.ts` | REST API for MoSE/MoLE |
| `cli.ts` | CLI commands for `mose` and `lora` |

### State format

State is float32 blob from `LlamaContextSequence.saveStateToFile()`. For 2.9B Goose model: ~21 MB as raw float32.

### Expert creation flow

```
1. Save current sequence state (restore point)
2. Load baseline state
3. Evaluate expert text through model
4. Save resulting state as _expert_<name>.state
5. Restore original state
```

## Verification

- [x] TypeScript compiles (`pnpm typecheck` passes)
- [x] State blending algorithm (weighted float32 sum of N binary files)
- [x] Expert creation follows existing `bakeSystemPrompt` save/load pattern
- [x] LoRA switching wraps existing `_setLoras` private API
- [x] CLI integration (mose + lora subcommands)
- [x] Gateway integration (REST endpoints)

## Future

- **Content-adaptive routing**: Tiny classifier predicts blend weights from user input
- **True MRSS**: Parallel sequences with logit-level gating
- **Trainable experts**: State-tuning via gradient descent on expert state vectors
- **State compression**: PCA or quantization of expert states for memory efficiency
