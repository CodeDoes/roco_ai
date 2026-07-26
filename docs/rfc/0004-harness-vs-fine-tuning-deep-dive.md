# RFC 0004: Harness Engineering vs Weight Updates
Status: Architectural Finding

## Empirical Benchmarks & Trade-Offs
- **Harness Engineering Gap:** +15-25% task accuracy delta achieved purely through retry loops, structured schema (kbnf/GBNF), context compaction, and verifiers.
- **Fine-Tuning Delta:** <5% accuracy improvement on equivalent model sizes (>3B). Fine-tuning is only cost-effective for sub-3B models requiring fixed DSL output formats to reduce prompt token overhead.
- **Recommendation:** Prioritize stuck-state detection, deterministic tool sandboxing, and token-level BNF grammar constraints over weight retraining.
