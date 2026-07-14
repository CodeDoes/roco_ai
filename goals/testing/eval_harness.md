# Testing

Intent: Eval harness, oracle management, and regression gates that keep the
inference engine honest as it grows.

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
| `message/system_instruction` | System prompts | ✅ Done |
| `message/tool_calling` | Tool call format | In progress |

## Implementation plan

### Phase 1: Oracle expansion
- Add 20+ new eval cases covering the gaps above
- Add grammar-strict validation (every token must be grammar-valid)
- Add throughput baseline capture (write to `evals/results/baseline.json`)

### Phase 2: Regression gate
- `roco eval --regression` compares against blessed oracles, exits non-zero
  on any failure
- `roco bless` updates all oracles and the baseline

### Phase 3: CI integration
- GitHub Actions (or local cron) runs `roco eval --regression` nightly
- Reports score deltas, flags model changes
