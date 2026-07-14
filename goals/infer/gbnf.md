# GBNF Grammar-Constrained Decoding

Intent: Restrict the model's output at every sample step to a GBNF grammar
(e.g. JSON/tool-schema) so generated text is always well-formed.

Reference: `web-rwkv-axum/src/components/transformer/bnf_constraint.rs`
