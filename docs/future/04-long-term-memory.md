# Long-Term Memory

Beyond state checkpoints — persistent cross-session knowledge.

## Current (Working)

- State checkpoints save full conversation context
- System prompt baseline remembers identity
- `_plan.md` tracks goals across sessions

## Future

### State Archive
Directory of named state files with metadata:
```
memory/
├── 2026-06-01_character-intro.state
├── 2026-06-02_plot-twist.state
├── 2026-06-03_worldbuilding-session.state
└── index.json  # title, summary, tags, date
```

### Retrieval
- Automatic: agent decides when to checkpoint
- On-demand: "remember when we discussed X" → load relevant state
- Scheduled: periodic summary state saves

### State Compression (Stretch)
- RWKV state is 21MB at 2.9B — manageable for dozens of checkpoints
- For hundreds, compress via state distillation (train a smaller state encoder)
- Goal: compress entire 100k-token story into a single state vector

### Cross-Session Knowledge
- Character profiles, setting details, plot threads persist across sessions via state archive
- Not RAG — state loading reconstructs the exact neural context
- No embedding, no vector search — just file lookup by metadata
