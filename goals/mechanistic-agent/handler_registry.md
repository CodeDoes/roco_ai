# Handler Registry

Intent: A typed map of (type, domain) → HandlerFn. Each mode registers its handlers; the router dispatches tasks to them. Unknown pairs fail loud instead of letting the model improvise. Handlers may call the model (grammar-constrained) or execute purely in code.

Sub-goals:
- Registration API: modes declare handlers at init time
- HandlerFn signature: `(task, workspace, actions) → Result`
- Model-calling handlers: handlers may invoke the model with a domain-specific grammar
- Code-only handlers: handlers that execute purely in classic code (validation, formatting, etc.)
