# Design Decisions

## Why RWKV Instead of Transformers

**Decision:** Use RWKV RNN architecture instead of transformer-based models.

**Trade-offs:**
- (+) Fixed-size state (~21MB) means no context window management. True infinite context.
- (+) State save/load is instant — serialize 21MB state vector, not growing KV cache.
- (+) State blending — additively compose multiple fine-tuned states.
- (-) Smaller ecosystem than LLaMA/GPT. Fewer tools, fewer trained adapters.
- (-) RWKV-PEFT training pipeline less mature than Axolotl/Unsloth.
- (-) 2.9B model limits quality vs 7B+ transformers.

**Verdict:** Correct decision for this use case (agent with persistent state). Transformer KV cache at 8K+ context would exceed available VRAM. RWKV fits in 4GB VRAM with infinite context.

## Why node-llama-cpp Instead of Python Inference

**Decision:** Use `node-llama-cpp` for in-process inference instead of Python subprocess.

**Trade-offs:**
- (+) Same language as the rest of the stack (TypeScript)
- (+) Direct `saveStateToFile`/`loadStateFromFile` access (critical for state management)
- (+) LoRA support at C++ addon level
- (+) No inter-process overhead, no serialization
- (-) `node-llama-cpp` lags behind llama.cpp releases
- (-) LoRA API not exposed in TypeScript types (requires `any` cast)
- (-) Python RWKV ecosystem has more training tools

## Why Separate State Files Instead of Single JSON

**Decision:** Store session state as JSON file + binary state files.

**Rationale:**
- JSON for human-readable message history (editable, debuggable)
- Binary for model state (fast load/save, compact)
- Separation allows loading old state without message history, or reloading messages into fresh state
- Binary state files are cross-platform, same format as llama.cpp

## Why Baked System Prompt State

**Decision:** Process system prompt once, save state, reuse forever.

**Why:**
- Every generation starts from same system prompt context without re-processing
- Saves ~100-200 tokens per generation (significant over thousands of calls)
- System prompt can be swapped mid-session by loading a different baseline
- Enables "mode switching" (prose vs planning vs tool-use) by loading different baselines

## Why Not RAG

**Decision:** No vector database or retrieval-augmented generation.

**Why:**
- RWKV state IS memory. The recurrent state vector encodes context.
- For story writing, the relevant context is the preceding narrative, captured in state.
- RAG adds complexity (embedding model, vector DB, retrieval pipeline) for marginal benefit.
- State checkpoints ARE your "retrieval" — load a previous chapter's state to recall details.

## Future: Mixture-of-State

**Decision:** Plan for state blending instead of model merging.

**Why:**
- RWKV state vectors exist in the same latent space
- Weighted sum: `s_blended = w₁·s₁ + w₂·s₂ + w₃·s₃`
- Train N mode states (prose, planning, tool-use, coding) → blend at inference time
- Router (tiny RWKV or MLP) predicts blend weights from input
- No need for LoRA switching or model reloading
