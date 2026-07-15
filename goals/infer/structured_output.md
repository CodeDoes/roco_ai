# Structured Output

Intent: Guarantee machine-parseable structured output (JSON/objects) via
JSON-Schema→GBNF conversion and grammar-constrained decoding. **Every** LLM
call in the agent pipeline uses a grammar — never free-form JSON extraction.

## Core principle: no free-form JSON extraction

The model **never** emits unconstrained text that the harness has to parse with
heuristics (`extract_first_json`, regex tricks, etc.). Every production is
governed by a BNF grammar applied at every token-sampling step:

```
Planner::plan()  ──via──▶  plan_gbnf()  ──▶  BnfConstraint transform  ──▶  valid JSON plan
Agent::run_step()  ──via──▶  response_gbnf()  ──▶  BnfConstraint transform  ──▶  valid assistant msg
Tool dispatch  ──via──▶  tool_schema_to_gbnf()  ──▶  BnfConstraint transform  ──▶  valid tool-call object
```

If the grammar rejects the emission, the sample is re-drawn — the model never
sees an invalid state. This makes the entire control flow deterministic:
a plan is guaranteed to be parseable, a tool call is guaranteed to have the
right shape, and the harness code never deals with malformed JSON.

## Grammar sources

| Source | Coverage | Example |
|---|---|---|
| `message_format_gbnf()` | System/User/Assistant chat messages with `<think>` and `<tool_call>` blocks | Conversation turns |
| `assistant_response_gbnf()` | Assistant-only output (after System/User context already in prompt) | Step responses, plans |
| `jsonschema_to_gbnf()` | JSON Schema → GBNF converter for ad-hoc output shapes | Tool argument schemas |
| Domain grammars | Per-task BNF productions (plans, chapters, wiki entries, events) | Story mode sub-plans |

## What We Learned from Live Generation

During multi-step story pipeline runs on undertrained RWKV models (1B–2.9B), the **no free-form JSON extraction** principle was proven correct:

- **System prompts alone cannot prevent meta-commentary** — `<think>` tag leakage persists regardless of instruction strength
- **Temperature decay has minimal effect** — contamination occurs at all temperatures
- **Post-processing is fragile** — models often never close their think tags, making regex-based stripping unreliable
- **Grammar-constrained decoding is the only reliable solution** — the sampler rejects non-conforming tokens at every step, so contamination literally cannot occur

### Domain grammars are non-negotiable

Every stage handler needs its own BNF grammar:
- Outline handler → `outline.bnf` (title, chapters, genre, tone)
- Wiki handler → `wiki.bnf` (characters, locations, lore)
- Chapter handler → `chapter.bnf` (prose with proper narrative structure)
- Validation handler → `validation.bnf` (pass/fail criteria, issues list)
- Synopsis handler → `synopsis.bnf` (single-paragraph summary)

The current story pipeline uses **pre-fill workarounds** (`<thinking>plan</think>` before prompt) as interim measures. These are explicit signals that domain-specific grammars are still needed.

### Pre-fill pattern for interim use

When grammars aren't available yet, pre-filling a think block tricks the model into clean output:
```
prompt = "<thinking>Plan: write outline...</think>\n\nWrite outline for: {premise}"
```
The model believes it already did thinking and continues directly into the requested content. This works because undertrained RWKV completes whatever follows a `<think>` marker.

## Sub-goals

- **Plan grammar** (`plan_gbnf`): `<task-list>` with typed tasks, dependencies,
tool references; emitted by `Planner::plan()` under constraint
- **Per-tool grammars**: Each registered tool's argument schema compiles to a
  GBNF fragment so tool calls are structurally valid before the model speaks
- **Response grammar**: `assistant_response_gbnf()` envelopes think tags, tool
calls, and plain text within strict boundaries
- **Eval-driven validation**: every token produced in a grammar-constrained
  turn must match — verified by grammar eval cases, not post-hoc parsing
- **Fallback safety**: if a grammar cannot cover a rare construct, tighten the
  prompt or fall back to a subset of the schema rather than accepting free-text
