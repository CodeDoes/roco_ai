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

## Reference

`web-rwkv-axum/src/components/transformer/bnf_constraint.rs`