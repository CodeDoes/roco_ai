# Workspace Sandbox

Intent: A request-scoped temp directory the model writes into. The model never sees the real filesystem — the controller moves artifacts from the sandbox into durable storage via actions.

Sub-goals:
- Temp directory lifecycle: create per request, discard after commit
- Write-only model access: model emits content, controller places it in the workspace
- Workspace → actions mapping: controller reads artifacts, dispatches to action handlers
- Cleanup: discard workspace after commit to prevent state leakage

Reference: `mindful/agent/engine.py` — `Workspace` class wraps `tempfile.mkdtemp()`; model writes only through `ws.write()`. `ksr/spec.md` — "Temp-workspace lifecycle (`Temp-<slug>/`) is ksr's."
