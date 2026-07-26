# RFC 0009: Unfiltered Local Creative Generation
Status: Policy & Implementation Note

## Design Principles
- Local inference eliminates external commercial content filtering and telemetry.
- Quality and formatting validation managed deterministically via `Verifier` rules (e.g. `forbidden_words`, outline structure checks) rather than model-level refusal prompts.
- Users maintain full control over `Verifier` parameters for creative fiction, roleplay, and worldbuilding tools.
