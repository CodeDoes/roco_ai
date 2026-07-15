# User Message Response

Intent: Produce a correct, on-topic response to a user turn — the core conversational exchange.

Sub-goals:
- On-topic coherence: reply addresses the user's question directly
- Structural correctness: output conforms to the message GBNF grammar
- Length appropriateness: response length matches the prompt scope
- Eval probes: baseline measurement via `user_turn_coherence` case

Reference: `crates/engine/src/cases.rs` — `user_turn_coherence` eval case. `crates/message/src/format.rs` — prompt builders.
