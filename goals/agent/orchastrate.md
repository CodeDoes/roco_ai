# Orchestrate

Intent: Execute plan steps in dependency order, coordinating tool dispatch,
model subtasks, self-prompting chains, and eval-based verification — with
dynamic subtask injection based on result complexity.

## Wave-level execution

Steps are grouped into dependency waves (`Plan::wave_levels()`): steps whose
dependencies are all satisfied run together (potentially in parallel). After each
wave completes:

1. **Verify**: Run step outputs through relevant eval cases
2. **Decide**: Check if results are sufficient or need refinement
3. **Expand** (optional): If complexity exceeds threshold, inject subtasks
4. **Continue**: Next wave begins with updated context

```
Wave 0: [step_1(read_file), step_2(fetch_deps)]
    ↓ both complete → verify outputs
    ↓ step_2 result has unexpected format
    → generate_subtasks(step_2_result) → [step_2a(parse_json), step_2b(validate_schema)]
    → inject into next wave
Wave 1: [step_3(write_code), step_2a, step_2b]
    ↓
Wave 2: [step_4(run_tests)  ← depends on step_3 + step_2a,b]
    ↓ done
```

## Subtask injection strategy

| Trigger | Action | Grammar |
|---|---|---|
| Step output malformed | Repair: retry with tightened params | Same as step grammar |
| Result incomplete / low quality | Generate 1–3 subtasks via model call | `plan_gbnf()` again |
| Complexity threshold exceeded | Auto-decompose current step | Same plan schema |
| Eval check failed | Add verification subtask | Verification-specific grammar |

Subtask generation is a **model call constrained to the plan grammar** — the same
GBNF that produced the original plan. It receives partial results as context and
emits additional structured steps:

```
Prompt: "Previous steps completed:\n- Step 1: outline generated\n         - Step 2: draft chapter (confidence 0.6)\n        Step 2 needs refinement. What substeps?\nEmit JSON plan:"
Output: {"task":"refine chapter 2","steps":[{"id":"2a","description":"extract key points"},{"id":"2b","description":"rewrite with missing details"}]}
(Successfully deserializes under plan_gbnf constraint)
```

## Configurable mechanistic depth

The orchestration loop accepts a **depth parameter** controlling how granular the
agent gets:

| Depth level | Behavior |
|---|---|
| Shallow (1) | Execute plan as-is, no verification, no subtasks |
| Medium (2) | Verify each step against evals, accept/reject binary |
| Deep (3+) | If rejection, auto-inject subtasks; recursive expansion up to N levels |
| Autonomous | Self-prompting chain runs until all evals pass or budget exhausted |

This is controlled by `AgentConfig.depth_level` and `AgentConfig.max_expansion_rounds`.

## Sub-goals

- **Wave scheduler**: Dependency-aware execution using Kahn's algorithm (already done — `Plan::topological_order()`) and wave-level grouping (`wave_levels()`)
- **Eval verifier integration**: Hook into step completion to run inline verification before advancing waves
- **Subtask generator**: Grammar-constrained model call that takes partial results → emits additional plan steps
- **Depth configuration**: AgentConfig field controlling decomposition granularity and subtask injection behavior
- **Budget-aware expansion**: Stop injecting subtasks when token budget or max_steps threshold approaches
