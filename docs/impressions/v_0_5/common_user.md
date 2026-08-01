# Common User Impressions — v0.5

## Test Date
2026-07-31

## Test Method
Simulated common user through tmux terminal session. User has no technical background, just wants to write stories.

## Test Commands
```bash
roco --help
roco -p "Hello"
roco story --help
roco session --help
roco workspace --help
```

## User Journey

See `/tmp/user_journal.md` for detailed journey log.

### Key Observations

1. **Session and workspace are confusing terms**
   - Common users don't know what these mean
   - They add cognitive load
   - They're not needed for simple use cases

2. **The router is the best UX**
   - Natural language input
   - Auto-detection of intent
   - No commands to remember

3. **The -p flag works well**
   - One-shot prompts are intuitive
   - "roco -p 'write a story'" feels natural

4. **Help text is too technical**
   - "Session management" → what's that?
   - "Workspace" → what's that?
   - Better: "Continue your stories" and "Story projects"

### What Works

- `roco -p "write a story"` — magical, just works
- The router with natural language — intuitive
- Emoji progress indicators — satisfying

### What Confuses Users

- `session` and `workspace` terminology
- Long help text with too many options
- No clear "quick start" path
- No auto-management of state

### Recommendations

The plan from this test (rename `session`→`chat`/`continue`, `workspace`→`project`; auto-management; simplified help) has been adopted into AGENTS.md — see §12 (workflow + UX items) and §13 (router NLU plan, the single origin). This section is the historical record only; AGENTS.md is canonical.

## User Score: 6/10
The magic works when users discover the router or `-p` flag, but the technical terminology (`session`, `workspace`) creates friction. The NLU router is the golden opportunity — it already handles natural language, we just need to extend it with more intents.
