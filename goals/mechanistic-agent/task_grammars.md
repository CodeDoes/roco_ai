# Task Grammars

Intent: BNF grammars that constrain model output per task domain (plan, chapter, wiki, synopsis, etc.). Derive uses the plan grammar; per-type handlers use their own domain grammar. Output that doesn't parse is caught by the repair loop.

Sub-goals:
- Plan grammar: `<task-list>` with typed tasks
- Per-domain grammars: chapter prose, wiki entry, synopsis, event log, etc.
- Grammar per mode: each mode declares its task set with corresponding grammars

Reference: `mindful/spec/agent.md` — plan grammar BNF definition with `<task>`, `<type>`, `<domain>`, `<spec>` productions. `ksr/spec.md` — typed tasks for plan/chapter/wiki.
