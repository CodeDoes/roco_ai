# State Mixing

Intent: Blend or combine multiple saved states (e.g. persona/context fusion)
to steer generation by merging prior conditionings. And more broadly: support
**multiple concurrent conversation states** on constrained VRAM via a state
pool with LRU eviction.

## Reference: web-rwkv-axum InferPool (Prunoideae)

The `web-rwkv-axum` repo implements a complete state-pool system under
`src/components/state/`:

### NamedState
- Wraps an `AxumBackedState` with an ID
- Supports `load_to(pool, slot)` (named → GPU slot) and
  `back_from(pool, slot)` (GPU slot → named)
- Can be serialized/deserialized to disk via CBOR (`serde.rs`)
- Can be cloned (deep copy of underlying state)

### InferPool
- Fixed-size GPU state pool (`AxumModelState::new_sized(batch_size)`)
- LRU cache mapping slot indices → `InferState`
- `InferRequest` = `NamedState` + token receiver + logits callback
- Slots:
  1. Check if state is already loaded in a slot (cache hit → promote in LRU)
  2. Find empty slot (not in cache) → load state into it
  3. Evict least-recently-used state → back it out, load new one
- `infer()` call runs **all active slots** in a single `model.run()` batch
- Inference loop blocks on all active requests for tokens, maximizing batch size

### Key insight for roco_ai

RWKV states are **swappable** — the recurrent hidden state can be:
1. Loaded from RAM into a GPU slot (`load_to`)
2. Read back from GPU into RAM (`back_from`)
3. Cloned for state forking (branch a conversation)
4. Serialized to disk for persistence
5. Merged/blended with another state (true "mixing" — weighted average of
   hidden state tensors)

This means on a 4 GB RTX 2050, you can maintain dozens of conversations in
RAM, with only 2–3 occupying GPU slots at any moment. States swap in/out
per inference step with minimal copying overhead.

## What roco_ai needs

### Phase 1: State pool (single-session, but swappable)
- Define a `StateSlot` backed by the existing `AnyState` in `rwkv_backend.rs`
- Implement `save_state()` / `load_state()` (serialize to disk, or keep in RAM)
- Add a single-slot pool: save state before switching contexts, restore after

### Phase 2: Multi-slot pool (concurrent conversations)
- N-slot GPU pool (N = 2–3 on RTX 2050, bounded by `state_size × N × VRAM`)
- LRU cache mapping conversation IDs → slot indices
- `load_state(id, slot)` / `back_state(id, slot)` via tensor copy
- Session management API: `create_session(id)`, `switch_session(id)`,
  `close_session(id)`

### Phase 3: State mixing (blend)
- Tensor-level blending: `mixed[i] = α·state_a[i] + (1-α)·state_b[i]`
- Use cases: persona fusion, context interpolation, warm-start from cached
  persona states
- Requires RWKV state tensor layout knowledge (from `web-rwkv` internals)

## Constraints
- VRAM: each state on 2.9B NF4 occupies ~1.4 GB; at most 2 slots fit on
  RTX 2050 simultaneously
- Copy bandwidth: `load_to`/`back_from` is a full tensor copy per switch;
  the InferPool pattern amortizes this by batching all active slots in one
  `model.run()` call
- web-rwkv version: our vendored `web-rwkv` may need patches to expose
  state load/back primitives; check `vendor/web-rwkv/` API surface first

## User notes
- State-mixing is the key enabler for multi-session agent use (each tool call,
  each conversation branch gets its own state without blowing VRAM)
- The AI00 server achieves this with an LRU state pool + continuous infer loop
  that blocks on all active slots — we should adopt the same pattern
