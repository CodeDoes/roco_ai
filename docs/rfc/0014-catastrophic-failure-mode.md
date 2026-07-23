# RFC 0014: Catastrophic Failure Modes
Status: Safety Critical
If MockBackend returns empty string: agent.verify fails immediately. If rollback attempts exceed max_retries: StackResult.success = false. If Sandbox detects path escape: returns Err immediately without file access. If Context.memory exceeds 100 entries: oldest entry dropped (LRU). System never crashes on mock failure.
