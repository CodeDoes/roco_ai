# RFC 0007: Stuck-State Detection and Rollback
Status: Implemented
Algorithm: If verify() fails for max_attempts consecutive iterations, state.attempts increments, checkpoint updates, and history logs the rollback. Loop exits after max_retries (default 3). Final StackResult reports rollback_count. If rollback_count > 0 but success = false, system flags persistent failure requiring harness redesign rather than retry.
