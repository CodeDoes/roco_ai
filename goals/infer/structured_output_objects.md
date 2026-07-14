# Structured Output: Objects & Arrays

Intent: Extend the JSON-Schema → GBNF converter (`jsonschema_to_gbnf`) to emit object (inline KV) and array productions, which it currently rejects with `BadSchema`. Closes the obvious forward extension of `structured_output`.

User: Object/array support is the known next step for structured output; schoolmarm accepts inline KV rules (see the converter file comment). No current eval case demands it yet.
