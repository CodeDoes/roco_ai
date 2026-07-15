# Tool Execution Loop

Intent: Execute a structured plan step-by-step under classic Rust control flow,
using grammar-constrained model calls at each iteration. Tools are dispatched
deterministically; results feed into the next prompt via self-prompting chain.

## Two execution modes

### Mode 1: ReAct (open-ended, model-driven iteration)
```
Loop:
  LLM produces think + tool_call OR final_answer (GBNF constrained)
  parse_assistant_response() → segments
  if tool_calls: execute each → append to history → loop again
  else: return final_text
```
The model decides how many iterations it needs. Token budget and max_steps cap it.

### Mode 2: Plan-first (predetermined, code-driven iteration) ← preferred for complex tasks
```
1. Planner::plan(task) → Plan { steps: [...] } via constrained grammar
2. For each step in topological_order():
   a. Build prompt: system + prior_results + current_step
   b. LLM call or tool dispatch (both grammar-constrained)
   c. Capture result
   d. Verify against eval cases
   e. Optionally inject subtasks → loop continues
```
Classic code owns the iteration count, order, and termination.

## Self-prompting chain

Each completed step becomes context for the next query. The model sees its own
previous outputs as structured data — not free narrative:

```rust
// Built automatically by the harness
let prompt = format!(
    "System: {}\n\nPlan task: {}\n\nResults from prior steps:\n{}\n\n\
     Now perform step {}: {}\nResult:",
    config.system_prompt,
    plan.task,
    prev_results.iter().map(|(id, out)| format!("- [step {id}] {out}")).join("\n"),
    step.id,
    step.description
);
```

No external controller tells the model what to do next — it reads its own output
from the previous turn plus the plan description and produces the next step's
result. This is **self-prompting**: the pipeline prompts itself through the
structured state.

## Eval verification integration

After each step completes, the harness can run inline checks:

```rust
let verified = eval_verifier.check(&step.description, &result);
if !verified {
    // Option A: trigger repair loop (tighten params, retry same step)
    // Option B: generate subtasks (ask model for additional steps)
    // Option C: accept with warning (low-stakes step)
}
```

Evals serve dual purposes:
1. **Regression gates** (post-hoc): `roco eval` suite ensures behavior doesn't degrade
2. **Inline verification** (during execution): step outputs validated before proceeding
