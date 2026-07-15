# GBNF Grammar-Constrained Decoding

Intent: Restrict the model's output at every sample step to a GBNF grammar
so **every token** is guaranteed valid — no post-hoc parsing, no free-form
JSON extraction, no heuristic recovery. The grammar controls the sampling
distribution directly via the vocabulary trie.

## How it works in the agent pipeline

Every LLM call uses exactly one of these grammars:

| Call site | Grammar | What it constrains |
|---|---|---|
| `Planner::plan()` | plan GBNF (dedicated or schema→GBNF) | `{"task":…,"steps":[{…}]}` — structured plan |
| `Agent::run()` step turn | `assistant_response_gbnf()` | think tags, tool calls, plain text |
| Tool argument emission | per-tool schema compiled to GBNF | specific key shapes, types, enums |
| Subtask execution | same as step turn | deterministic response structure |
| Self-prompting chain | whatever the previous step's grammar was | consistent shape across iterations |

## Line-by-line constraint philosophy

Instead of emitting unconstrained text and extracting JSON later:

```
❌ "Here's my plan: {"tasks": [...]} maybe?"
   → extract_first_json() tries to find braces
   → fragile: fails if model adds commentary

✅ BNF root ::= task-list forces: every line starts with [
                  → every token is pre-validated
                  → serde_json::from_str() succeeds 100% of the time
```

This applies to tool calls too: each tool's argument schema compiles to a
GBNF fragment embedded in the response grammar, so the model can only produce
valid tool-call objects — never malformed ones that need regex repair.

## What We Learned from Live Generation

During multi-step story pipeline runs on undertrained RWKV models (1B–2.9B), the **grammar-first principle** was proven correct and non-negotiable:

### The contamination problem
- **System prompts alone cannot prevent meta-commentary** — `<think>` tag leakage persists regardless of instruction strength
- **Temperature decay has minimal effect** — contamination occurs at all temperatures
- **Post-processing is fragile** — models often never close their think tags, making regex-based stripping unreliable
- **Grammar-constrained decoding is the only reliable solution** — the sampler rejects non-conforming tokens at every step, so contamination literally cannot occur

### Domain grammars are critical
Every stage handler needs its own BNF grammar, not just the plan/tool grammars:
- Outline handler → `outline.bnf` (title, chapters, genre, tone structure)
- Wiki handler → `wiki.bnf` (characters, locations, lore fields)
- Chapter handler → `chapter.bnf` (prose narrative structure)
- Validation handler → `validation.bnf` (pass/fail criteria, issues list)
- Synopsis handler → `synopsis.bnf` (single-paragraph summary)

### Pre-fill pattern for interim use
When domain grammars aren't available yet, pre-filling a think block tricks the model into clean output:
```
prompt = "<thinking>Plan: write outline...</think>\n\nWrite outline for: {premise}"
```
The model believes it already did thinking and continues directly into the requested content. This is an **explicit signal** that a domain-specific grammar is still needed.

### Architecture decision
**Every model call must go through a BNF grammar.** This is the fundamental guarantee that separates controllable generation from gambling on undertrained models. The story pipeline currently uses pre-fill workarounds as interim measures — these should be removed once all stages have proper domain grammars.

## Reference
`web-rwkv-axum/src/components/transformer/bnf_constraint.rs`