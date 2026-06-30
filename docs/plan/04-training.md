# Phase 4: Training Pipeline

## Goal
Train custom LoRAs and state-tuned adapters for specific agent behaviors.

## Milestones

### 1. Storytelling LoRA (Current Priority)
- Dataset: 100-500 prose examples in target style
- Platform: Runpod RTX 4090
- Duration: ~15 min
- Precision: nf4 (fits 5.7GB)
- Output: `.gguf` LoRA adapter (~84MB)
- Verification: `pnpm tell --lora=adapters/prose.gguf`

### 2. Tool-Use LoRA
- Dataset: tool-call jsonl examples (50-200)
- Trains model to output `<tool_call>` format reliably
- Combined with prose LoRA via multi-LoRA loading

### 3. State Tuning for Modes
- Train initial state vectors per mode (prose, planning, tool-use)
- Faster than LoRA (~2 min per mode)
- Lighter output (state file, not adapter weights)
- Enables mixture-of-state blending

### 4. Kaggle Pipeline
- Notebook-based training for free tier
- 30hr/week GPU quota sufficient for 1hr training runs
- Automatic artifact download to local

## Dataset Format

```jsonl
{"role": "user", "content": "write chapter 2"}
{"role": "assistant", "content": "<tool_call>{...}</tool_call> prose text here"}
```

## Conversion Pipeline
```
.pth (RWKV-PEFT output)
  → convert to GGUF (llama.cpp tool)
    → load with --lora in harness
```
