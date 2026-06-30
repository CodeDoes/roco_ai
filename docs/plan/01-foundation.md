# Phase 1: Foundation

## Status: In Progress ✓

### Completed
- RWKV engine with Vulkan GPU acceleration
- State save/load (baseline + named checkpoints)
- System prompt baking into latent state
- Storytelling agent with chapter/scene management
- CLI with 7 commands (tell, chapter, checkpoint, plan, interactive, continue, state-info)
- LoRA adapter loading (via `--lora`)
- Paragraph break workaround (`--fix-paragraphs`)
- Session persistence (messages + metadata)

### Remaining
- [ ] True streaming (token-by-token, not batch)
- [ ] Wire file tools into agent loop
- [ ] Tool-call parsing and execution
- [ ] GBNF grammar support for structured output
- [ ] Fix find.ts/grep.ts path bugs
- [ ] Test suite (vitest)
