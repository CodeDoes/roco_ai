# Tool Catalogue

Intent: Maintain the registry/discovery of available tools the model may call, with their schemas.

## Current state (2025-07-14)

- `roco-tools::Tool` trait: `name()`, `description()`, `schema()` (JSON Schema), `call(args)`.
- `roco-tools::ToolRegistry`: `register()`, `get()`, `names()`, `len()`.
- `roco-tools::builtins::all_tools()` returns 6 built-ins: `read`, `write`, `search`, `list`,
  `bash`, `now`. Each implements `Tool` with a JSON-Schema `schema()`.
- The agent loop collects `schema()` for every registered tool and passes them to
  `message_format_gbnf()` so the `<tools>` block enumerates available tools.

## Status: DONE
