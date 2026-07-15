# Goals: mechanistic-agent

A plugin layer on top of the core agent loop. The mechanistic agent replaces the model-driven agent loop with a **code-driven controller + router** — the model is a subroutine called only at fixed, grammar-constrained points; classic code owns all control flow, dispatch, error handling, and iteration logic. Every LLM call produces BNF-valid output — no free-form JSON extraction ever.

## Core philosophy

```
Grammar owns:      structural validity — what output CAN look like (BNF per stage)
Code owns:         iteration order, dependency graphs, error paths, budgets, subtask injection
Model provides:    content filling within grammar boundaries only
Result:            deterministic control flow over structurally guaranteed generation
```

### Grammar-First Principle
**Every model call must go through a BNF grammar.** This is not optional infrastructure — it's the
fundamental guarantee that separates controllable generation from gambling.

Free-form prompting (no grammar) on small RWKV models produces:
- Meta-commentary (`thinking>` tags, planning text, "let me...")
- Structurally invalid JSON or prose
- Inconsistent formatting across stages

With grammar constraints:
- The sampler rejects non-conforming tokens at every step
- No post-processing stripping needed — contamination literally cannot occur
- `serde_json::from_str()` always succeeds
- Error recovery reduces to timeout/retry only

All stages in the mechanistic agent pipeline (plan → outline → wiki → chapter × 3 → validate → synopsis → publish)
should have domain-specific grammars. The current story example uses pre-fill workarounds as interim measures,
signaling where proper grammars are still needed.

Prerequisite order (top to bottom):

1. **self_controlled_ingest** — the controller decides what the model reads; context is pulled, not pushed
2. **intent_classification** — classify user input → route + mode selection; low confidence falls back to `justChatting`; intent schema is BNF-constrained
3. **task_grammars** — BNF grammars per task domain; model output is structurally trusted; covers plans, chapters, wiki entries, AND tool-call argument schemas
4. **workspace_sandbox** — request-scoped temp directory; model never touches the real filesystem
5. **controller** — predetermined mode selection: constrained plan emission → classic Rust execution loop with eval verification gates and self-prompting chain
6. **router** — (type, domain) → handler dispatch table; unknown pairs fail loud; modes register handler maps at init
7. **mode_file_format** — the `.mode` DSL: route, system, model, tools/tasks, state, loop, exit_codes, examples
8. **modes** — declarative route definitions (system prompt, tools, model size, state, workflow loop); router dispatches here; each mode bundles its own task grammars
9. **fallback_chains** — modes declare fallback chains; low confidence or retry exhaustion routes to the next mode; terminal fallback is always `justChatting`
10. **handler_registry** — typed (type, domain) → HandlerFn map; modes register handlers; unknown pairs fail loud; handlers may call the model (grammar-constrained) or run purely in code
11. **state_mounted_instructions** — system instructions keyed by content hash, mounted per mode, not in-prompt; enables prompt efficiency across long multi-step runs
12. **repair_loop** — grammar validate, structure oracle, retry with tightened params, fallback; bounded retries then fallback via fallback_chains
13. **actions_gate** — actions as the only exit to durable state; three-gate safety model (grammar gate → schema gate → action gate)
14. **trace_observability** — per-turn structured logs capture the full controller trace for debugging, replay, and eval; includes grammar validity scores, subtask injection events, and eval verification results
