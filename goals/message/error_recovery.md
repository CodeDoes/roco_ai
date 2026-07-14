# Error Recovery

Intent: Detect and recover from malformed constrained-decoding failures, timeouts, or parse errors (retry, fallback, or graceful abort).

## Current state (2025-07-14)

- `roco-message::error::RetryConfig` controls `max_retries`, `base_delay`, grammar fallback,
  and truncation shortening.
- `roco-message::error::complete_with_retry()` implements: backend-error retry with exponential
  backoff; grammar-error fallback to unconstrained generation; truncation retry with reduced
  `max_tokens`.
- `roco-message::error::InferenceError` enumerates grammar/timeout/truncation/tool/backend errors.
- `roco-message::error::is_truncated_response()` detects unclosed `<think>`/`<tool_call>`/`<tool_result>` tags.
- `roco-message::error::describe_error()` produces a human-readable message for UX.
- `roco-engine::MockBackend` supports `fail_count` for testing the retry path.

## Status: DONE
