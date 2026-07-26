# RFC 0006: Security Boundary Model
Status: Critical / Enforced

## Security Rules
1. **Path Boundary Enforcement:** Path traversal prevented via `is_safe_relative_path()`. Rejects absolute paths, root prefixes, and `..` relative traversals.
2. **File Size Cap:** Standard 10MB maximum file size limit in `Sandbox::read`/`write`.
3. **Extension Whitelist:** Restricted to safe readable/writable extensions (`txt`, `md`, `json`, `py`, `rs`).
4. **Air-Gapped Isolation:** Zero outbound network calls allowed in agent execution loop. Backend operates locally without remote API fallback.
