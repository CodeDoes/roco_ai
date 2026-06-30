# Mixture of State Experts (MoSE) + Mixture of LoRA Experts (MoLE)

RWKV's fixed-size recurrent state enables something impossible with transformers: **state blending**. Blend N expert states at the binary level (weighted float32 sum) → single forward pass with combined behavior.

## Architecture (Rust Inference API)

State blending is handled server-side by the Rust inference API (`rwkv-inference-api`). Expert states are registered via HTTP as base64-encoded state tensors and blended in GPU memory on the Rust server.

```
rwkv-harness ──▸ rwkv-inference-api (Rust, axum, Vulkan)
                     │ POST /v1/mose/expert   register expert state
                     │ POST /v1/mose/blend    blend experts server-side
                     │ POST /v1/mose/generate blend + generate
                     │ POST /v1/generate      standard generation
                     │ GET  /v1/state         export current state (base64)
                     │ POST /v1/state         import state (base64)
```

### Backends

| Backend | Engine | Usage |
|---------|--------|-------|
| **Rust API** (default for MoSE) | `RwkvApiEngine` | `--api=http://localhost:3100` |
| llama.cpp (legacy) | `RwkvEngine` | default (no `--api` flag) |

## How State Blending Works (Rust API)

The Rust API server stores expert states as raw `TensorCpu<f32>` in memory. On blend:
1. N expert states read as `&[f32]` from hashmap
2. Element-wise weighted float32 sum in Rust (`blend_states` function)
3. Normalized by total weight
4. Loaded into engine's active state via `state.load()`
5. Single forward pass with blended state

The same HTTP interface means rwkv-harness never touches raw state bytes. Everything is base64 over POST/GET.

## MoSE CLI Usage

Use `--api` flag to connect to the Rust inference API instead of local llama.cpp:

```bash
# Start API server (separate terminal)
cd /home/kit/dev/rwkv-inference-api
./target/release/rwkv-inference-api --model=model.st --quant=32 --port=3100

# Create expert (state baked from text, stored server-side)
pnpm tsx cli.ts --api=http://localhost:3100 mose expert create formal \
  --text="You write with academic formality. Precise vocabulary, structured paragraphs."

pnpm tsx cli.ts --api=http://localhost:3100 mose expert create creative \
  --text="You write with vivid imagery. Metaphors, sensory detail, varied sentence rhythm."

# List experts
pnpm tsx cli.ts --api=http://localhost:3100 mose expert ls

# Blend and generate
pnpm tsx cli.ts --api=http://localhost:3100 mose generate "Explain quantum computing" formal=0.7 creative=0.3

# Standard generate (no blend)
pnpm tsx cli.ts --api=http://localhost:3100 tell "Write a story about AI"

# Gateway mode with API backend
pnpm tsx cli.ts --api=http://localhost:3100 gateway
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

When running `pnpm tsx cli.ts --api=http://localhost:3100 gateway`, the gateway forwards MoSE requests to the Rust API:

| Method | Path | Body | Description |
|--------|------|------|-------------|
| POST | `/mose/experts` | `{name, text, weight?}` | Create expert from text (eval + register via API) |
| GET | `/mose/experts` | | List experts |
| DELETE | `/mose/experts/:name` | | Remove expert |
| POST | `/mose/blend` | `{weights: {name: w, ...}}` | Blend experts server-side |
| POST | `/mose/generate` | `{prompt, blend?, ...genOpts}` | Blend then generate |
| POST | `/mose/segment` | `{segments: [{text, blend}]}` | Segment routing |

## Rust Inference API Endpoints

The Rust server (`rwkv-inference-api`) exposes:

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Server health + state size |
| POST | `/v1/tokenize` | `{text}` → `{tokens}` |
| POST | `/v1/detokenize` | `{tokens}` → `{text}` |
| POST | `/v1/eval` | `{tokens}` → `{logits}` (updates state) |
| GET | `/v1/state` | Export state as base64 |
| POST | `/v1/state` | Import state (base64) |
| POST | `/v1/state/clear` | Reset state to zeros |
| POST | `/v1/generate` | `{prompt, max_tokens, temperature, top_p}` → `{text, tokens_generated}` |
| POST | `/v1/mose/expert` | `{name, state (base64), weight?}` → register expert |
| GET | `/v1/mose/expert/list` | List registered experts |
| DELETE | `/v1/mose/expert/{name}` | Remove expert |
| POST | `/v1/mose/blend` | `{weights: {name: w, ...}}` → blend states, load into engine |
| POST | `/v1/mose/generate` | `{prompt, blend?, max_tokens, temperature, top_p}` → blend + generate |

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

## Implementation Details

### Rust API files

| File | Role |
|------|------|
| `/home/kit/dev/rwkv-inference-api/src/main.rs` | Axum HTTP server, 14 endpoints, MoSE state blending |
| `/home/kit/extern/web-rwkv/src/runtime/loader.rs` | Patched for Goose V7 variant (`num_head = num_emb / v`) |
| `/home/kit/extern/web-rwkv/src/runtime/v7.rs` | Patched: `r_k` tensor reshaped to `[head_size, num_head]` |
| `/home/kit/dev/convert_gguf_to_st.py` | GGUF→ST converter for web-rwkv V7 format |

### State format

State is `TensorCpu<f32>` with shape `[num_emb, head_size + 2, num_layer, 1]` = `[2560, 66, 32, 1]` for 2.9B Goose model. Total: 21,626,880 bytes (~20.6 MB) as raw float32, ~28 MB as base64 over HTTP.

### Expert creation flow (Rust API)

```
1. GET /v1/state → save current state (restore point)
2. POST /v1/state/clear → reset to zeros
3. POST /v1/eval {tokens} → evaluate expert text
4. GET /v1/state → export expert state as base64
5. POST /v1/mose/expert {name, state, weight} → register
6. POST /v1/state {state} → restore original state
```

### True MRSS (future)

The Rust API's `back(0)` / `load(tensor, 0)` state operations support true MRSS: maintain N expert states in CPU memory, evaluate each independently, combine logits per token. Not implemented but the state API is ready.

## Verification

- [x] TypeScript compiles (`pnpm typecheck` passes)
- [x] State blending algorithm (weighted float32 sum of N binary files)
- [x] Expert creation follows existing `bakeSystemPrompt` save/load pattern
- [x] LoRA switching wraps existing `_setLoras` private API
- [x] CLI integration (mose + lora subcommands)
- [x] Gateway integration (REST endpoints)
- [ ] End-to-end with real model on RTX 2050 4GB (needs Q4_K_M GGUF)

## Future

- **Content-adaptive routing**: Tiny classifier predicts blend weights from user input
- **True MRSS**: Parallel sequences with logit-level gating
- **Trainable experts**: State-tuning via gradient descent on expert state vectors
- **State compression**: PCA or quantization of expert states for memory efficiency
