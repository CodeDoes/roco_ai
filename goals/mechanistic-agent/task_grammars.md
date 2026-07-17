# Task Grammars

## Intent
BNF grammars that constrain model output per task domain (plan, chapter, wiki, synopsis, etc.).
Derive uses the plan grammar; per-type handlers use their own domain grammar. 
Output that doesn't parse is caught by the repair loop.

## Why This Matters: Learnings from Live Generation
### The contamination problem proved grammars are non-negotiable
During multi-step story pipeline runs on a 2.9B RWKV model, we observed:
- **System prompts alone cannot prevent** `thinking>` tag leakage or meta-commentary
- **Temperature adjustment has minimal effect** — the model leaks planning text at any temperature
- **Free-form prompting produces structurally unreliable output** — outline/wiki/validation/synopsis all contaminated
- Post-processing stripping is fragile and incomplete when models don't close their think tags

**Grammar-constrained decoding is the only reliable solution.** When output must satisfy a BNF grammar:
- The sampler rejects non-conforming tokens at every step
- No meta-commentary can slip through — it's not part of the grammar
- No post-processing needed — output is structurally guaranteed
- `serde_json::from_str()` always succeeds on constrained output
- Error recovery reduces to timeout/retry logic only

### Grammar design principles
- Each stage handler should have its OWN domain grammar (outline grammar ≠ wiki grammar ≠ chapter grammar)
- Grammars should be small but precise — enough structure to prevent ambiguity, not so much they block valid content
- JSON-compatible: derive plans → JSON parser, chapters → prose templates, wiki → structured fields
- Use `json_schema_to_gbnf` (`crates/grammar/src/json_schema.rs`) to auto-generate from schema objects

## Sub-goals
- **Plan grammar**: `<task-list>` with typed tasks — DONE in message GBNF, needs integration into MechanisticAgent
- **Per-domain grammars**: chapter prose, wiki entry, synopsis, validation report, event log
- **Grammar per mode**: each mode declares its task set with corresponding grammars
- **Auto-generation**: `json_schema_to_gbnf()` converts Rust schema types to GBNF at compile time

## Integration Status
- ✅ BNF infrastructure: `BnfConstraint` (`crates/grammar/src/bnf.rs`) + `bnf_sampler` v0.3.8
- ✅ GBNF→BNF converter wraps nonterminal names in angle brackets
- ✅ JSON-Schema → GBNF converter (`crates/grammar/src/json_schema.rs`) handles objects, arrays, enums
- ✅ Story-domain per-handler grammars (`GBNF/*.bnf`: outline, wiki, chapter_prose, validation_report, synopsis) — embedded + validated via `roco_grammar::grammar_library` against `roco-bnf-engine`
- ⬜ Story pipeline stages wired to BNF constraints (current workaround: pre-fill think blocks)
- ⬜ Per-handler grammars in mechanistic agent — this is the next critical gap

## Reference
- `mindful/spec/agent.md` — plan grammar BNF definition with `<task>`, `<type>`, `<domain>`, `<spec>` productions
- `ksr/spec.md` — typed tasks for plan/chapter/wiki
- `crates/grammar/src/bnf.rs` — `BnfConstraint` implementation with vocab trie
