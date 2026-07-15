# Testing

Intent: Eval harness, oracle management, and regression gates that keep the
inference engine honest as it grows. Evals serve **dual purposes**: post-hoc
regression testing AND mid-execution verification gates for the plan-and-execute loop.

## Inline eval verification (mid-execution checks)

Evals are not just for post-hoc regression testing — they serve as **verification**
**gates during agent execution**:

```
Plan step completes → result captured
    ↓
StepVerifier::check(step_description, result) → EvalCheck {
    passed: bool,
    confidence: f32,
    issues: Vec<String>
}
    ↓
passed=true  → advance to next step
passed=false → trigger repair or subtask injection
```

This means every step output is tested against known-good behavior before the
plan proceeds. If a step produces a malformed plan fragment, a poorly formatted
chapter, or an incorrect code generation, the inline verifier catches it immediately
rather than letting errors compound downstream.

### Implementation approach

```rust
pub struct StepVerifier {
    // Maps step descriptions to relevant eval case IDs
    steps_by_category: HashMap<String, Vec<EvalCaseId>>,
}

impl StepVerifier {
    pub fn check(&self, step_desc: &str, result: &str) -> EvalCheck {
        let cases = self.steps_by_category.get(category_for(step_desc));
        match cases {
            Some(cases) => run_eval_cases(cases, result),  // structured verdict
            None => EvalCheck { passed: true, confidence: 0.5 }, // no criteria yet
        }
    }
}
```

Eval categories map to step types:
- `tool_dispatch` → format correctness of JSON output
- `code_generation` → compiles / lint passes
- `prose_generation` → structural completeness (has intro, body, conclusion?)
- `plan_fragment` → valid GBNF parseable JSON shape

### Phase 1 priorities
- StepVerifier scaffold with category mapping
- Basic eval cases per step type (structural, not behavioral)
- Wire into Plan::execute() as optional middleware gate
- Integration tests showing failed step blocks progression

## What exists today

- **`eval_suite.rs`** — runs `EvalCase` records against any `ModelBackend`
  (mock, rwkv). Categories: smoke, instruction, coherence, format, throughput.
- **Snapshot/bless workflow** — every `roco eval` saves `.snapshot.json`;
  `roco bless` updates oracle fields.
- **Grammar eval fixtures** — `grammar_eval_cases()` (hand-written GBNF)
  and `jsonschema_eval_cases()` (JSON Schema → GBNF chain).
- **114 unit tests** across the workspace.

## What's missing

### Oracle suite expansion
- More eval cases covering tool calling, error recovery, multi-turn coherence
- Throughput regression gate (fail if tok/s drops below baseline on the 2.9B)
- Grammar-constrained eval cases that verify *every* token matches the grammar
  (not just the first/last)

### Regression automation
- CI gate: `roco eval` → compare against blessed oracles → fail on regression
- Model version gate: when a new model is dropped into `models/`, re-run the
  full suite and flag any score drops
- Quantization gate: compare NF4 vs Int8 vs no-quant on a fixed prompt set

### Test quality
- Current eval cases are small (~10). Need 50+ covering edge cases:
  - Unicode / CJK / emoji handling
  - Very long prompts (context window stress)
  - Grammar edge cases (nested JSON, recursive structures)
  - Tool call parsing (malformed tool calls, partial completions)
  - State persistence across sessions (does loading a session reproduce the
    same output?)

## Dependencies

| Dep | Goal | Status |
|---|---|---|
| `infer/inference` | RWKV inference engine | ✅ Done |
| `infer/streaming` | Token streaming | ✅ Done |
| `infer/gbnf` | Grammar-constrained decoding | ✅ Done |
| `infer/state_mixing` | Session state pool | ✅ Phase 1 done |
| `infer/structured_output` | No free-form JSON | ✅ Done |
| `agent/planning` | Structured plan decomposition | ✅ Done |
| `agent/self_prompting` | Self-prompting chain | 🟡 New |
| `message/tool_calling` | Constrained tool calls | ✅ Done |

## Implementation plan

### Phase 1: Oracle expansion
- Add 20+ new eval cases covering the gaps above
- Add grammar-strict validation (every token must be grammar-valid)
- Add throughput baseline capture (write to `evals/results/baseline.json`)

### Phase 2: Regression gate
- `roco eval --regression` compares against blessed oracles, exits non-zero
  on any failure
- `roco bless` updates all oracles and the baseline

### Phase 3: Inline verification
- StepVerifier scaffolding with category-to-eval-case mapping
- Wire into Plan::execute() as optional middleware gate
- Verify each step output against relevant cases before advancing waves

### Phase 4: CI integration
- GitHub Actions (or local cron) runs `roco eval --regression` nightly
- Reports score deltas, flags model changes
