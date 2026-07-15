# Planning

Intent: Convert natural-language input into a **grammar-constrained structured**
plan that a classic code loop can execute deterministically — one task at a time,
with optional subtask injection and eval-based verification.

## The predetermined mode selection flow

```
System instruction + User message
    ↓ (prompt already includes system prompt as prefix)
┌─────────────────────────────────────────────┐
│  LLM CALL 1 — Plan emission                 │
│  Grammar: plan_gbnf() — structurally valid   │
│  JSON only; no commentary possible            │
│                                             │
│  Output:
│  {                                             │
│    "task": "Write a story about cats",        │
│    "steps": [                                │
│      {"id":"1","description":"outline plot",\ │
│       "tool":"write","depends_on":[]},       │
│      {"id":"2","description":"draft chapter1",│
│       "tool":"write","depends_on":["1"]}     │
│    ]                                           │
│  }                                            │
└─────────────────────────────────────────────┘
    ↓ (serde deserialization always succeeds)
┌─────────────────────────────────────────────┐
│  CLASSIC RUST LOOP                          │
│  for step in plan.topological_order():        │
│    result = dispatch(step)                    │
│    if !verify(result):                        │
│      subtasks = generate_subtasks(result)     │
│      plan.steps.extend(subtasks)              │
│    record_eval(step, result)                  │
│  end                                          │
└─────────────────────────────────────────────┘
    ↓
Final assembled output / report
```

## Key design properties

### No free-form intermediaries
The plan is emitted under BNF constraint. `serde_json::from_str()` on the model's
output always succeeds — there are no heuristics, no brace-counting fallbacks,
no "extract first JSON from noisy text". If the grammar cannot express what
the model wants to say, the prompt tightens rather than the parser relaxing.

### Classic code owns control flow
Once the plan exists, the Rust control loop determines execution order, handles
errors, injects subtasks, and decides when to stop. The model never chooses "what
step comes next" — it only provides the initial decomposition. This makes resource
usage predictable and debuggable.

### Self-prompting chain
Each completed step feeds its output back as context for the next query:

```
Prompt: "Plan task: X\nPrior results:\n- Step 1: [outline]\n\nNow do step 2: draft"
    → LLM emits constrained response
    → Result captured
    → Same pattern repeats with updated prior_results context
```
The model prompts itself through the chain by seeing its own previous outputs as
structured context. No open-ended reasoning needed between steps.

### Configurable mechanistic depth
The granularity of the initial plan depends on how complex you want the agent to be:
- **Shallow**: 1–3 top-level steps, simple tool dispatch
- **Medium**: 5–10 steps with dependencies, some require model-generated content
- **Deep**: Full decomposition with per-step verification, nested subtask loops
- **Autonomous**: Mid-loop planner trigger adds subtasks when step results need refinement

### Evals as verification gates
Every step output can be checked against eval cases:
```
for step in plan:
    result = execute(step)
    if matches_eval_case(step.description, result):
        pass ✓
    elif confidence_below_threshold:
        trigger_plan_recompilation() or repair()
    else:
        accept (human-judge step or low-stakes step)
```
Evals serve dual purposes: regression testing (post-hoc) and inline verification
(during execution).

## Sub-goals

- **Constrained plan grammar**: Dedicated GBNF for `{task, steps:[{id, description, tool, depends_on}]}` — replaces `Planner::plan()`'s free-text JSON extraction
- **Self-prompting prompt builder**: Automated assembly of `(system + accumulated_prior_results + current_step_description)` prompts for each iteration
- **Subtask generator**: Model call that takes partial results and emits additional plan steps (constrained to same plan grammar)
- **Eval verifier**: Integration point where step outputs run through relevant eval cases before proceeding
- **Depth configuration**: AgentConfig parameter controlling initial decomposition granularity and whether subtask injection is enabled
