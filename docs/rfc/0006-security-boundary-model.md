# RFC 0006: Security Boundary Enforcement
Status: Critical
Every workspace file access passes through Sandbox.read/write. Path escape attempts return Err("path escape blocked"). File size limits enforce 10MB cap. Allowed extensions restrict to safe types (txt, md, json, py, rs). No network calls permitted from agent execution context. MockBackend never connects to external URLs.
