# Controller

Intent: The core orchestration loop — think (free-form model call), derive (grammar-constrained model call → structured plan), dispatch each task to a handler, commit workspace artifacts to durable state.

Sub-goals:
- `think`: free-form model call to reason about the user request
- `derive`: grammar-constrained model call → structured task list
- `dispatch`: iterate tasks, route each to its handler
- `commit`: move workspace artifacts → durable state via actions
