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
