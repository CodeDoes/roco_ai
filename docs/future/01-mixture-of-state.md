# Mixture-of-State Router

RWKV's fixed-size state vector enables something transformers cannot do: **state blending**.

## Concept

Train N mode-specific state vectors (via state tuning):
- `s_prose` — narrative flow, sensory detail, pacing
- `s_planning` — structured thinking, outlines, organization
- `s_toolcall` — JSON precision, function-call formatting
- `s_coding` — code formatting, syntax correctness
- `s_steering` — following user instructions precisely

At inference, blend them:
```
s_final = α·s_system + β·s_prose + γ·s_planning + ...
```

## Router

A tiny model (0.1B RWKV or lightweight MLP) that:
1. Reads user input
2. Predicts blend weights (α, β, γ, ...)
3. Blends states → loads into main RWKV → generates

## Advantages

- **Zero extra inference cost** — blend once, generate normally
- **Continuous interpolation** — morph between modes smoothly
- **Per-turn switching** — different blend per user message
- **No model reloading** — state swap is instant
- **Trainable end-to-end** — router learns optimal blends from feedback
