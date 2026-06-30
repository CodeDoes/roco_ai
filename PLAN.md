# Project Plan

## Phase 1: Foundation ✓

- [x] RWKV engine with Vulkan GPU
- [x] State save/load (baseline + checkpoints)
- [x] System prompt baking into state
- [x] Storytelling agent
- [x] CLI with 7 commands
- [x] LoRA adapter loading
- [x] Paragraph break workaround
- [x] Session persistence

## Phase 2: Agent Loop (Current)

- [x] Tool-calling agent loop (`src/agent-loop.ts`)
- [x] Wire existing `tools/` into agent (`src/tool-registry.ts`)
- [x] Fix `find.ts` / `grep.ts` path resolution bugs
- [x] CLI `agent` command
- [x] True token-by-token streaming in `generateStream`
- [x] GBNF grammar for structured output enforcement (via `LlamaGrammarEvaluationState`)
- [x] Session checkpointing in agent loop (turn + pre-tool checkpoints)
- [x] `--depth=N` / `--grammar=PATH` CLI flags
- [x] Tested base RWKV 2.9B agent loop (see findings below)

## Phase 3: Gateway & Channels

## Phase 4: Gateway & Channels

- [ ] HTTP/WebSocket gateway
- [ ] Web channel (dashboard)
- [ ] TUI (terminal UI)
- [ ] CLI (current)
- [ ] REST API
- [ ] Channel-agnostic session routing

## Phase 5: Skills & Memory

- [ ] Skill system (modular tool directories)
- [ ] Long-term memory (state archiving + retrieval)
- [ ] Cron/scheduled tasks
- [ ] System prompt as routable state

---

## Base RWKV 2.9B Test Findings

Tested `agent` command end-to-end with base RWKV 7 2.9B Q4_K_M on Vulkan.

**What works:**
- Model loads, generates, streams per-token ✓
- With `User: / Assistant:` role markers + few-shot examples, model generates `<tool_call>` tags ✓
- Tool calls parse, execute, feed results back to model ✓
- GBNF grammar constrains output to valid tool call format ✓
- Session checkpointing saves state at each turn ✓

**What needs work (prompt iteration, not code):**
- Model hallucinates `<tool_result>` blocks (generates fake tool output). GBNF grammar prevents this.
- Model sometimes picks wrong tool name (`list` instead of `ls`, or `read` for everything)
- `--fix-paragraphs` causes repetition on long generations
- Without GBNF, model generates natural language + tool calls mixed, but tool call format can be malformed

**Next prompt experiments to try:**
1. GBNF grammar that allows optional natural language before/after tool call
2. Stronger few-shot examples (3-5 instead of 2)
3. Lower temperature (0.5) for more deterministic tool choice
4. Tool names in ALL CAPS in descriptions to stand out (LS, READ, etc.)

## Deferred: Cloud Training (On Hold)

Training put on hold until harness proves base RWKV limits. If agent loop works well with prompting alone, training may not be needed.

- **Dataset**: still researching ideal tool-use dataset
- **Tool**: RWKV-PEFT on Runpod RTX 4090 (~$0.09/run)
- **Keys needed**: Runpod ($10 credit), Hugging Face token, SSH keypair
- **Accounts**: runpod.io account + billing, huggingface.co token, ssh-keygen

## Immediate Next Steps

1. Fix `generateStream` — true token-by-token streaming
2. Add GBNF grammar for tool-call schema enforcement
3. Test base RWKV agent loop: can 2.9B call tools with good prompting?
4. Wire session checkpointing into agent loop
5. Print tool call count/depth in `agent` output
