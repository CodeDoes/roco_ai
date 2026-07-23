# RFC 0010: Air-Gapped RAG Architecture
Status: Design
Personal documents never leave machine. Session store writes to .roco/ directory only. Vector embeddings (if added) compute locally. No cloud vector DB permitted. Context.memory contains only user-approved snippets. MemoryStore implements RecallTool and RememberTool with explicit user consent per entry.
