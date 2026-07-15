# Task Grammars

Intent: BNF grammars that constrain model output per task domain (plan, chapter, wiki, lookup, etc.). Derive uses the plan grammar; per-type handlers use their own domain grammar. Output that doesn't parse is caught by the repair loop.

Reference: `mindful/spec/agent.md` — plan grammar BNF definition with `<task>`, `<type>`, `<domain>`, `<spec>` productions.
