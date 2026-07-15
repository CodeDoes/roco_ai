# Modes

Intent: Declarative route definitions that package system prompt, tool availability, model size, state shape, and workflow loop into a named mode. The router dispatches to a mode by (type, domain); modes can declare fallback chains and confidence thresholds for automatic rerouting.

| Mode | Description | Tasks | Tools |
|------|-------------|-------|-------|
| `justChatting` | Casual conversation; default fallback when confidence is low | — | none |
| `coder` | Generate, debug, refactor, and ship code | — | fs, shell, lint |
| `proseWriter` | Creative writing (fiction, poetry, dialogue) | — | none |
| `research` | Synthesize supplied material | — | — |
| `search` | Fetch live information | — | — |
| `adventureGame` | Solo text adventure engine | — | — |
| `trpg` | Tabletop RPG game master | — | — |
| `random` | Jokes, games, and light distractions | — | — |
| `worldBuilding` | Construct and maintain consistent fictional worlds | — | — |
| `storyTeller` | Story generation, tracking, and publishing — manages plot, events, wiki, synopsis, chapters, and a world bible | plot, events, wiki, synopsis, chapter, edit, validate, publish | — |

Concrete `.mode` files live in `mechanist_agent/modes/`. The `mode_file_format` goal defines the DSL.
