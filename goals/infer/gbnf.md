# GBNF Grammar-Constrained Decoding

Intent: Restrict the model's output at every sample step to a GBNF grammar
(e.g. JSON/tool-schema) so generated text is always well-formed.

## Current state

**Schoolmarm walker** (`rwkv_backend.rs`):
- Compiles a GBNF string into `schoolmarm::Grammar` per `complete()` call
- Masks disallowed token logits to `-inf` via `allowed_tokens()`
- Advances grammar state with `accept_token()` after each sample
- Works end-to-end but has known fragility: the walker can enter an invalid
  state from step 1 on simple grammars (e.g. `root ::= "yes" | "no"`) when
  the model's preferred first tokens don't align with the grammar

**Grammar generation** (`grammar.rs`, `jsonschema_to_gbnf.rs`):
- `tools_to_gbnf()` / `tools_to_gbnf_with_think()` / `message_format_gbnf()` —
  produce GBNF from tool schemas
- `jsonschema_to_gbnf::schema_to_gbnf()` — JSON Schema → GBNF for primitives
  + enums; objects/arrays rejected with `BadSchema`
- `validate_grammar()` — checks GBNF is well-formed and closed

**Plumbing**:
- `CompletionRequest::grammar: Option<String>` carried through the engine
- `RWKV_GRAMMAR` / `RWKV_GRAMMAR_FILE` env vars for scripts
- `grammar_smoke` example binary (yes/no binding test)
- `eval_suite::grammar_eval_cases()` + `eval_suite::jsonschema_eval_cases()`

## Next step: integrate `bnf_sampler`

The [bnf_sampler](https://crates.io/crates/bnf_sampler) crate (latest 0.3.8)
is a production-grade grammar sampler used by web-rwkv-axum (Prunoideae). It
improves on schoolmarm in several ways:

| | schoolmarm | bnf_sampler |
|---|---|---|
| Parsing | Simple walker | Recursive descent |
| Vocab awareness | Bytes → UTF-8 PUA mapping | `qp-trie` byte[] → token_id, full id→string reverse map |
| State | Opaque state object | Explicit stack arena with configurable capacity |
| Next-token set | Enumerates allowed tokens | `all_possible_next_tokens()` returns `BitSet` |
| Token rejection | Returns empty set (walker stuck) | `AcceptTokenResult::Failed` with error message |
| Reset | New grammar state | `sampler.reset()` clears stack |

Integration plan:
1. Add `bnf_sampler` + `qp-trie` + `bit-set` + `rustc-hash` to `Cargo.toml`
2. Build a `BnfConstraint` struct (mirroring `web-rwkv-axum`'s transformer pattern):
   - `new(grammar: &str, vocab: &Tokenizer)` → parses grammar, builds vocabulary trie
   - `update(prompt: &[u16])` → feeds prompt tokens through the grammar
   - `mask_logits(logits: &[f32]) -> Vec<f32>` → applies `BitSet` mask (disallowed = `f32::MIN`)
   - `accept_token(token_id: u32)` → advances grammar state, returns `Continue/Failed/End`
   - `reset()` → clears state for next request
3. Wire into `rwkv_backend.rs` as an alternative to the schoolmarm path
   (keep schoolmarm as fallback; `bnf_sampler` as the `grammar-rwkv` default)
4. Update `grammar_smoke` example + eval fixtures to exercise the new path

Reference: `web-rwkv-axum/src/components/transformer/bnf_constraint.rs`
