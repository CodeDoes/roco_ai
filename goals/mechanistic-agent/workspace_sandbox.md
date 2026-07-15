# Workspace Sandbox

Intent: A request-scoped temp directory the model writes into. The model never sees the real filesystem — the controller moves artifacts from the sandbox into durable storage via actions.

Reference: `mindful/agent/engine.py` — `Workspace` class wraps `tempfile.mkdtemp()`; model writes only through `ws.write()`. `ksr/spec.md` — "Temp-workspace lifecycle (`Temp-<slug>/`) is ksr's."
