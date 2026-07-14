# Structured Output

Intent: Guarantee machine-parseable structured output (JSON/objects) via
JSON-Schema→GBNF conversion and grammar-constrained decoding.

## Current state

- `jsonschema_to_gbnf::schema_to_gbnf()` converts JSON Schema → GBNF for
  primitives (string, number, boolean, null) and enums
- Objects/arrays are rejected with `BadSchema` (forward extension)
- `eval_suite::jsonschema_eval_cases()` exercises the full chain
- Grammar is enforced at inference time via schoolmarm GBNF walker

## Next step: bnf_sampler as the constraint engine

Replace schoolmarm with `bnf_sampler` for the structured-output path:
- `bnf_sampler` handles the GBNF → token constraint pipeline natively
- Vocabulary trie (`qp-trie`) maps byte sequences → token IDs directly
- `all_possible_next_tokens()` computes the valid set efficiently
- Failed token acceptance returns a clear error (vs. schoolmarm's silent
  invalid-state trap)

Object/array support in `jsonschema_to_gbnf` becomes worthwhile once
`bnf_sampler` is the engine — its recursive descent parser handles nested
productions correctly, so we can emit inline KV rules for objects and
array item rules without fear of the walker getting stuck.
