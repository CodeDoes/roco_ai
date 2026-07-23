# RFC 0004: Harness Engineering Dominates Weight Updates
Status: Evidence-Based
Claim: Harness quality gap = +15-25% task accuracy. Fine-tuning gap on same weights = <5% for large models. Fine-tuning only justified for sub-8B models requiring strict DSL output to save prompt token overhead.
Evidence: SWE-bench verified variance 34-48% across harnesses. Execution environment benchmarking shows 25+ point swings from context compaction and retry policies alone.
Recommendation: Invest in sandbox execution loops, stuck-state detection, strict schema enforcement (MCP), and sub-agent isolation rather than retraining.
