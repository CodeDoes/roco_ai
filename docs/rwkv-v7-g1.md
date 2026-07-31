# RWKV-7 G1H 2.9B — Capabilities & Limitations

## Model Overview

| Property | Value |
|----------|-------|
| Architecture | RWKV-7 (State Space Model) |
| Parameters | 2.9B |
| Layers | 32 |
| Embedding | 2560 |
| Heads | 40 |
| Head Size | 64 |
| Context | ~10k tokens (trained) |
| Quantization | Sandwich (2 edge FP16, 28 middle NF4) |
| Vocabulary | 65536 tokens |
| Hardware Target | Consumer GPU (8-16 GB VRAM) |

## Core Abilities

### 1. Recurrent State Management

RWKV-7 maintains a **recurrent state vector** (~130 MB FP32) that summarizes everything seen so far. Unlike Transformers, this state is:

- **Constant size** regardless of sequence length
- **Serializable** to bytes for persistence
- **Loadable** to continue from any point
- **Blendable** between states (weighted average)

**What this enables:**
- Save/restore story sessions without re-feeding history
- Multiple story branches from same state
- Chapter continuation with instant state load

### 2. Grammar-Constrained Decoding

RWKV-7 supports **token-level grammar masking** via kbnf/GBNF grammars. At each decoding step, disallowed tokens are zeroed from the probability distribution.

**Key characteristics:**
- Grammar is applied at the decoder, not post-hoc
- Works with JSON schemas converted to GBNF
- Critical for structured output on small models
- Requires proper enum scoping (see Known Issues below)

### 3. State Tuning (Baking)

" Baking" feeds few-shot examples through the model without generating, priming the recurrent state with format expectations.

**Pattern:**
```
bake([("User: Q1", "Assistant: A1"), ("User: Q2", "Assistant: A2")], session="format")
load_state("format")
complete(prompt)  // Model continues in learned format
```

**What baking enables:**
- Format consistency without repetition in prompt
- Style transfer between sessions
- Suppressing thinking tags by baking `</think>` state

### 4. Temperature & Sampling

| Temperature | Behavior |
|-------------|----------|
| 0.0 | Greedy — always picks highest probability |
| 0.3-0.5 | Good for structured output (JSON, outlines) |
| 0.5-0.7 | Good for prose (chapters, storytelling) |
| ≥0.7 | **Repetition starts** — avoid for this model |

**Critical finding:** The 2.9B model starts repeating at temperature ≥0.7. The vertical slice uses 0.5 for all phases.

## Known Limitations

### 1. Repetition Under Constraints

When grammar masks are active, in-string content can degrade:
- Generated tokens can become degenerate: `":[`, `s,s,s`
- Structure remains valid JSON, but content quality varies
- **Mitigation:** Revision retry loop converges after 2-3 attempts

### 2. Intermittent Empty Responses

~30-50% of requests can return empty completions, especially on:
- Simple prompts (smoke tests)
- Long prompts exceeding comfortable state capacity
- GPU memory pressure situations

**Not a system bug** — this is model-layer instability. The pipeline handles it via retries.

### 3. State Capacity Limits

With ~10k trained context:
- Full novel cannot fit in state alone
- Chapter 1-3 works because each is ~500-1500 words
- Wiki/outline act as **external memory** to compensate
- State carries format/style, not full narrative

### 4. Judge Literalism

The model judges instruction-following too literally:
- "crystal sword" ≠ "ancient artifact" → fails `follows_instructions` eval
- This is a model limitation, not a system bug
- **Mitigation:** Accept as known failure, focus on structural correctness

### 5. Name/Entity Consistency

Character names can drift between chapters:
- "Dr. Elena Voss" → "Dr. Elena Vance"
- Pronoun shifts ("her" → "his")
- **Mitigation:** Wiki provides explicit character refs; validation catches drift

## Evaluation Strategy

The eval suite tests these capability buckets:

| Category | Purpose | Key Evals |
|----------|---------|-----------|
| **Smoke** | Basic functionality | `smoke_basic_reply`, `smoke_empty_system` |
| **Instruction** | Following constraints | `instruct_format_constraint`, `instruct_step_by_step`, `instruct_negative` |
| **Coherence** | Quality output | `coherence_explain`, `coherence_story`, `repetition_avoidance` |
| **Format** | Structured output | `format_json`, `format_list` |
| **Grammar** | BNF constraint | Grammar eval cases |
| **Validation** | Pipeline phases | `val_*` cases |
| **Story** | End-to-end pipeline | `story_outline_json`, `story_wiki_json`, `story_chapter_json`, `story_validation_json` |

## Recommended Temperature Guide

| Phase | Temp | Reason |
|-------|------|--------|
| Outline | 0.5 | Structured, needs consistency |
| Wiki | 0.5 | Structured, needs consistency |
| Chapter (first pass) | 0.5 | Balanced creativity/coherence |
| Chapter (revision) | 0.5 | Same, revision prompt helps |
| Validation | 0.5 | Strict quality check |
| Synopsis | 0.5 | Summarization, not creative |
| Baking | 0.0 | Deterministic state priming |

## Grammar Design Notes

### GBNF `|` Precedence Trap

The `|` operator has the **lowest precedence** in GBNF/kbnf. Inlining enums into object rules splits the rule into multiple alternatives:

```bnf
# BAD - model can legally emit just "pass" without the object
root ::= "{" "quality": "pass" | "fail" | "needs-work" "}"

# GOOD - enum as named rule, referenced by object
root_quality_enum ::= "\"pass\"" | "\"fail\"" | "\"needs-work\""
root ::= "{" "quality": root_quality_enum "}"
```

### Prefill vs Grammar

Prefill tokens are fed to the model but NOT passed through the grammar mask. The mask re-emits the opening token (e.g., `{`), so `resp.text` contains complete JSON. Callers parse `resp.text` directly.

## Research Connections

- **DREAMSTATE** (arXiv:2601.19221): RWKV state is editable knowledge — relevant for state-mixing features
- **DeltaProduct** (arXiv:2502.10297): State-tracking expressivity limits — explains decoder stall at long contexts
- **GoldFinch** (arXiv:2407.12077): Hybrid RWKV+attention works at scale — validates future architecture paths
- **Revenge of the Fallen** (arXiv:2404.19178): Recurrent models match transformers on psycholinguistics — validates RWKV for narrative

## Implementation Notes

### State Pool
- Max 8 sessions (LRU eviction)
- Thread-safe via `Arc<RwLock<HashMap>>`
- FIFO replacement when full

### Two Named States in Pipeline
| Name | Purpose |
|------|---------|
| `story-writer` | Accumulates full context across phases |
| `story-validator` | Reset between chapters to prevent repetition bleed |

### Baking Patterns
```rust
// Bake story format
bake([("User: Write a story", "Assistant: Once upon a time...")], "story-writer")

// Bake validation format  
bake([("User: Review this", "Assistant: {\"quality\": \"pass\"}")], "story-validator")
```

## Future Improvements

1. **State compression** — LZ4/ZSTD to reduce 130MB state to ~30-50MB
2. **State monitoring** — entropy/norm metrics for debugging
3. **State editing** — DREAMSTATE-style diffusion for creative interventions
4. **Hybrid attention** — occasional attention layers for long-form coherence
