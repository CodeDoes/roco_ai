# Actions Gate

Intent: Sanctioned mutations (create, update, delete) are the only exit from the workspace into durable state. Every action emits a data-changed record. Combined with grammar and schema gates, this keeps the model from touching persistence directly.
