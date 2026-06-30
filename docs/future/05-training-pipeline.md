# Training Pipeline

## LoRA Training (Current Target)

```
Base model: RWKV-7 2.9B (HuggingFace)
Framework: RWKV-PEFT
Platform: Runpod RTX 4090 ($0.34/hr)
Duration: ~15 min per LoRA
Precision: nf4 (5.7GB VRAM)
Data: 50-500 jsonl examples

Pipeline:
1. Get .pth base model from BlinkDL/rwkv7-g1
2. Prepare jsonl dataset (tool calls, story format, etc.)
3. Deploy to Runpod:
   - git clone RWKV-PEFT
   - pip install -r requirements.txt
   - configure run_lora.sh
   - sh scripts/run_lora.sh
4. Download .pth LoRA adapter
5. Convert .pth → .gguf (llama.cpp conversion tool)
6. Load in harness: pnpm tell --lora=adapters/story.gguf
```

## State Tuning (Near Future)

Even faster than LoRA — trains only initial state vector.
- ~2 min on Runpod 4090
- Output: .pth state file (KB-MB)
- No merge needed — mount separately in Ai00 or merge into GGUF
- Better for behavioral steering, worse for format learning

## Dataset Generation

### For Tool-Use Training
```
{"role": "user", "content": "read chapter 3"}
{"role": "assistant", "content": "<tool_call>\n{\"name\": \"read\", \"args\": {\"path\": \"s/story/c/003_chapter.md\"}}\n</tool_call>"}
```

Generate synthetically using the model itself (bootstrapping) or hand-craft 50-100 examples.

## Kaggle Alternative

Free T4 GPU (16GB VRAM) with 30hr/week quota:
```
!git clone https://github.com/JL-er/RWKV-PEFT.git
!pip install -r requirements.txt
!python train.py ...  # same args, just notebook format
!cp output/*.pth /kaggle/working/
```

9hr session timeout — fine for sub-1hr training runs.
