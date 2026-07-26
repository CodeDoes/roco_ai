# RFC 0010: Privacy-Preserving Local RAG Architecture
Status: Design & Implemented Baseline

## Architecture Specs
- **Local Persistence:** Session history and vector/text memory stored exclusively under workspace directory (`.roco/`).
- **Air-Gapped Context:** `Context.memory` stores only locally parsed snippets.
- **Explicit Consent Tools:** Memory modifications require explicit tool execution (`RecallTool`, `RememberTool`) with explicit user-per-entry scope.
- Cloud vector database dependencies prohibited.
