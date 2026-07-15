# Mode File Format (.mode)

Intent: The `.mode` file DSL for declaring routes. Each file defines one mode with system prompt, tool/task availability, model size, state shape, workflow loop, exit codes, and inline examples.

Fields:
- **route** `<name>` — mode identifier, matches the filename
- **system** — system prompt string emitted on mode activation
- **model** — `small` | `mid` | `big` (hints for model size selection)
- **tools** / **tasks** — declared capabilities with descriptions
- **state** — JSON-like schema of the mode's working state
- **loop** — numbered workflow steps (intent → action → return)
- **exit_codes** — terminal states: `clean`, `blocked`, `review`
- **notes** — operational constraints
- **examples** — inline worked traces

Reference: `mechanist_agent/modes/*.mode` for concrete definitions.
