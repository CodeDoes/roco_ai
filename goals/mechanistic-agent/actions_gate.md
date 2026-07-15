# Actions Gate

Intent: Sanctioned mutations (create, update, delete) are the only exit from the workspace into durable state. Every action emits a data-changed record. Combined with grammar and schema gates, this keeps the model from touching persistence directly.

Sub-goals:
- Grammar gate (model boundary): model output must parse against its task grammar
- Schema/vocab gate (workspace boundary): artifacts must conform to expected schemas
- Actions gate (durable boundary): only registered actions can mutate durable state
- data-changed event: every action emits an audit record

Reference: `mindful/spec/actions.md` — action signatures, lifecycle, block mapping, and the `data-changed` event. The three gates: grammar (model boundary), schema/vocab (workspace boundary), actions (durable boundary).
