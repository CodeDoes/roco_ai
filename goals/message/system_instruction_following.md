# System Instruction Following

Intent: Reliably obey system-level instructions and role/behavior prompts across a conversation.

Sub-goals:
- Single-turn adherence: model follows persona/format constraint in a single exchange
- Multi-turn persistence: persona holds across successive turns without reinforcement
- Negative constraints: model respects "do not X" instructions
- Eval probes: baseline measurement via `instruct_baseline_persona` case

Reference: `crates/engine/src/cases.rs` — `instruct_baseline_persona`, `instruct_negative`, `instruct_step_by_step` eval cases.

User: Can use examples with system instructions to have the model understand intent. Real goal is to figure out the baseline the model understands without prior state-tuning
