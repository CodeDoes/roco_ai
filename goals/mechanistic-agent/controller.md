# Controller

Intent: The core deterministic orchestration loop — no free-form text between
model calls, no heuristic parsing. Every model interaction uses a grammar;
every control-flow decision lives in classic Rust code. The model fills content
slots; the controller owns iteration order, error handling, and subtask injection.

## The pipeline: predetermined mode selection

```
Input: system_instruction + user_message
    ↓
┌──────────────────────────────────────┐
│  Phase 1: Plan emission              │
│  Grammar-constrained LLM call        │
│  → Structured plan {steps:[...]}     │
│  → serde deserialization always OK   │
└──────────────────────────────────────┘
    ↓
┌──────────────────────────────────────┐
│  Phase 2: Deterministic execution    │
│  Classic Rust loop over steps        │
│  - tool_dispatch for registered tools│
│  - model_subtask for reasoning steps │
│  - eval_verify after each step       │
│  - subtask_inject if verification fails│
└──────────────────────────────────────┘
    ↓
Output: assembled result / report
```

## Key design properties

### No free-form intermediaries
Every LLM call produces grammar-constrained output that serializes directly into
Rust types. No `extract_first_json()`, no regex recovery, no "try your best" parsing.
If the grammar rejects the token stream, sampling re-draws before the model speaks.

### Model as subroutine, not driver
The model never decides "what happens next." It only provides:
- Initial decomposition (plan steps)
- Content generation per step (chapters, code, analysis)
- Subtask expansion when complexity demands it

Control flow — iteration count, dependency ordering, error paths, termination —
is all classic Rust code. This makes resource usage predictable and debuggable.

### Self-prompting chain
Each completed step feeds its result back as context for the next query. The model
sees its own previous outputs as structured data through the automatically-assembled
prompt template. No external orchestrator tells it what comes next — the plan lists
it and the harness executes it.

### Eval gates at every stage
Plan validity → grammar parse (already guaranteed by BNF)
Step results → inline eval verification before advancing waves
Final output → regression eval suite for post-hoc validation

## Two execution modes

| Mode | Loop driver | When to use |
|---|---|---|
| ReAct | Model decides stop condition | Open-ended tasks, low complexity |
| Plan-first | Code iterates over fixed steps | Complex tasks, known sub-tasks |

Both use self-prompting chains and grammar-constrained output. The difference is
whether iteration count is model-driven or predetermined.

## Sub-goals

- **Constrained plan derivation**: Replace `Planner::plan()`'s free-text extraction with dedicated plan GBNF
- **Self-prompting prompt builder**: Auto-assemble `(system + prior_results + current_step)` prompts
- **Eval verifier integration**: Inline check gates between wave execution phases
- **Subtask generator**: Grammar-constrained model call for mid-loop step injection
- **Depth configuration**: AgentConfig parameter controlling decomposition granularity
