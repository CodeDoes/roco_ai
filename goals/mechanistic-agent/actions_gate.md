# Actions Gate

Intent: Sanctioned mutations (create, update, delete) are the only exit from the workspace into durable state. Every action emits a data-changed record. Combined with grammar and schema gates, this keeps the model from touching persistence directly.

Reference: `mindful/spec/actions.md` — action signatures, lifecycle, block mapping, and the `data-changed` event. The three gates: grammar (model boundary), schema/vocab (workspace boundary), actions (durable boundary).
