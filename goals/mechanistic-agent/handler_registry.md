# Handler Registry

Intent: A typed map of (type, domain) → HandlerFn. Each mode registers its handlers; the router dispatches tasks to them. Unknown pairs fail loud instead of letting the model improvise. Handlers may call the model (grammar-constrained) or execute purely in code.
