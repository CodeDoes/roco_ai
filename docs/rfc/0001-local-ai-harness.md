# RFC 0001: Local AI Harness Architecture
Status: Implemented / Architectural Baseline

## Core Architecture
Separates execution environment from model weights. Harness manages context, tool sandbox, verifiers, rollback loops, and state tracking.

## Core Interface Specs
- `DomainHarness`: Trait defining `name()`, `init()`, `run(context)`, `verify(output)`, `rollback(state)`.
- `ExecutionLoop`: Retries up to `max_retries` (default 3) on verification failure with state rollbacks.
- `Sandbox`: Confined file workspace access enforcing path boundary checks (`is_safe_relative_path`).
- `Verifier`: Deterministic checks (`forbidden_words`, `required_words`, `min_length`).
- `MockBackend`: Local fallback format string generator when RWKV backend is offline.
