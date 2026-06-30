# Mode System

Each mode is a distinct state-tuned latent state or LoRA adapter. Modes are blended at inference via mixture-of-state.

## Defined Modes

| Mode | Purpose | Training Data | Priority |
|------|---------|---------------|----------|
| System | Base persona, always active | None (hand-crafted prompt) | P0 |
| Prose | Narrative richness, sensory detail | Literature excerpts | P0 |
| Planning | Structured thinking, outlines | Story plans, outlines | P1 |
| Tool Call | JSON/function-call precision | Tool-use examples | P1 |
| Coding | Code formatting, syntax | Code snippets, markdown | P2 |
| Precision Editing | Surgical text modifications | Diff examples | P2 |
| Recall | Remember forgotten details | Q&A on story context | P2 |
| Note Taking | Structured note formatting | Obsidian/markdown notes | P2 |
| Research | Web research synthesis | Research summaries | P2 |
| Browser Use | Web navigation | Web interaction logs | P3 |
| Computer Use | Desktop automation | GUI action sequences | P3 |
| Terminal | Shell command generation | Terminal session logs | P3 |
| Log Processing | Parse and summarize logs | Log file examples | P3 |
| Real-time Response | Low-latency replies | Short response pairs | P3 |
| User Steering | Follow instructions precisely | Instruction-following data | P1 |
| User Interrupt | Handle mid-generation changes | Interruption examples | P2 |
| Fill-in-the-Middle | Context-aware completion | Infill examples | P2 |

## Mode Selection

Router determines active modes from:
1. Explicit: `--mode planning`
2. Implicit: detect intent from user input
3. Default: prose + system always active

## Implementation Priority

1. **System** — baked baseline (done)
2. **Prose** — storytelling LoRA (next)
3. **Planning** — structured outline mode
4. **Tool Call** — agent loop with tool use
