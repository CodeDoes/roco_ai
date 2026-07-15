# Self-Prompting Chain

Intent: The model prompts itself through a structured pipeline — each step's output becomes the context for the next query, without any external controller dictating what comes next. The loop body is classic Rust code; the LLM only fills in the content slots.

## How it works

```
Step N completes → result captured as structured data
    ↓
Harness builds prompt: "Prior results:\n{step_1_result}\n{step_2_result}...\nNow do: {step_N+1_description}"
    ↓
LLM call with grammar constraint → produces constrained output
    ↓
Output captured → becomes part of "prior results" for next iteration
    ↓
Repeat until plan exhausted or subtasks added
```

No human or orchestrator says "now do X." The **plan** lists steps in order. The **harness loop** iterates. Each iteration:
1. Reads all prior step outputs from memory
2. Constructs the prompt with system instruction + prior results + current step description
3. Calls the model under grammar constraint
4. Captures and verifies the result

The model never sees an empty slate — it always has its own previous outputs to reason about. This creates a **chain of self-prompted reasoning** where each link is structurally bounded by the grammar.

## Why this matters

| Without self-prompting | With self-prompting chain |
|---|---|
| External controller tracks state | State lives in the prompt history |
| Controller must assemble context manually | Harness assembles it automatically |
| Easy to lose track of what was done | Every step reads its predecessors |
| Hard to add mid-stream subtasks | Subtask injection fits naturally into the loop |
| Model has no memory of past turns | Model sees full context each turn |

## Prompt template

The harness auto-generates prompts like this:

```rust
let mut prompt = String::new();
prompt.push_str(&format!("System: {}\n\n", config.system_prompt));
prompt.push_str(&format!("Plan task: {}\n", plan.task));
prompt.push_str("\nResults from prior steps:\n");
for (id, out) in prior_results {
    prompt.push_str(&format!("- [step {}] {}\n", id, out));
}
prompt.push_str(&format!(
    "\nNow perform step {}: {}\nResult:",
    step.id, step.description
));
// Grammar constrains everything after "Result:"
```

This is deterministic assembly — no branching logic, no external state. The **grammar** is what makes it safe: even though the model sees accumulated context, its output at each step conforms to the expected schema.

## Self-prompting in different modes

### Plan-first mode (predetermined)
```
Planner emits → [step_1, step_2, ..., step_N]
Loop: for each step → self-prompt with prior results → execute → verify
Subtasks? If verification fails → emit more steps via plan_gbnf()
```

### ReAct mode (open-ended)
```
Loop: self-prompt with history → model decides: think, tool_call, or final_answer
Model chooses its own stop condition (no final tool_calls → done)
Budget limits iterations instead of plan length
```

Both use self-prompting chains — the difference is whether the **loop structure** is predetermined (plan-first) or model-driven (ReAct).

## Sub-goals

- **Auto-prompt assembler**: Function that takes `plan + prior_results + current_step` → builds prompt string with correct grammar delimiter
- **Context window management**: When prior_results exceed context budget, summarize or truncate oldest entries before next call
- **Confidence scoring**: Model optionally emits per-step confidence alongside result; low confidence triggers subtask generation
- **Checkpoint & resume**: Serialize prior_results to disk so chain can be resumed after interruption
- **Eval-based gating**: Before advancing to next step, run result through relevant eval cases; block advancement if check fails
