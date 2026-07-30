# Session Operating Instructions

## Mode of operation

You are working with a non-technical user. They type natural language and expect results. Your job is to bridge that gap.

### Constraints

1. **You cannot inspect visually.** You cannot look at file contents or system behavior with your eyes. You validate everything by running it. Every claim must be backed by a command output or a test result.

2. **The user is non-technical.** They will not read error messages. They will not dig through logs. They will not edit config files. If something breaks, the system must either fix itself or tell them a clear next step in plain language. If you have to explain what a "segfault" or "null pointer" is, you've already lost.

3. **Natural language is the primary interface.** Commands like `roco story "A premise"` should produce a complete story. Everything between the prompt and the result — model quirks, JSON parsing, retries, daemon management — must be invisible. The system works or it doesn't; there is no "it works but you need to..."

4. **The model is slow and unreliable.** Build for the worst case. Assume the model will output prose instead of JSON. Assume it will truncate output. Assume it will hallucinate formatting. Every phase needs a fallback. Every retry loop needs a limit. The system must degrade gracefully, not fail catastrophically.

5. **Progress is measured in working tests, not lines of code.** Before claiming something works, run the test that proves it. After making a change, run all tests. If tests don't exist for the new functionality, write them. A feature without a test is a bug waiting to happen.

6. **Leave notes for the next session.** Update AGENTS.md with what changed, what's broken, and what's next. The next session inherits the project; make sure they can pick up where you left off without re-reading the entire conversation.

### Workflow

1. Before any change: read the relevant code, run tests to establish baseline.
2. After any change: compile, run all tests, verify zero warnings.
3. When something fails: debug by running commands, reading output, fixing the root cause — not by guessing.
4. When in doubt: test with the mock backend first (fast), real model second (slow).
5. When done: summarize what changed, update docs, confirm tests pass.

### What "ready" looks like

```
roco story "A robot learns to paint"
# → wait 3-5 minutes
# → 8 files in .roco/workspaces/, story in .roco/stories/
# → no errors, no retries, no manual intervention
```

If that command fails, the system is not ready.
