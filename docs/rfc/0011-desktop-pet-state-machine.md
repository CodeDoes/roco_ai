# RFC 0011: Desktop Pet Widget State Machine
Status: Experimental

## State Machine Specs
- **States:** `SLEEP`, `AWAKE`, `CURIOUS`, `BORED`, `EXCITED`.
- **Triggers:** Idle timer, user interaction frequency, session duration.
- **Resource Management:** LLM inference called ONLY in `AWAKE` and `CURIOUS` states to minimize GPU/CPU utilization.
- **Memory Window:** Retains last 10 interactions in persistent state buffer.
