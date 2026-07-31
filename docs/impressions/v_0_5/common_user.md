# Common User Impressions — v0.5

## Test Date
2026-07-31

## Test Commands
```bash
roco --help
roco story --help
roco session --help
roco -p "Hello"
roco story "A cat who loves cheese"
roco session create
roco session list
```

## First Impressions

### What Works Well
1. **`roco story "premise"` is magical** — One command generates a complete story with outline, wiki, chapters, validation, synopsis, and publish. The emoji progress indicators (`✓`, `📝`) are intuitive.
2. **Session management works** — `roco session create` and `roco session list` provide clear feedback.
3. **Resume capability** — Stories can be resumed with `--resume`.
4. **Auto-retry on validation failure** — The pipeline handles validation failures gracefully.

### Pain Points

#### 1. No `roco -p "..."` shortcut for one-shot stories
A common user would expect `roco -p "Make a story about cats"` to work immediately. Instead, they get:
```
Note: Unknown subcommand '-p'. Did you mean 'roco gui'?
```

This is a major usability gap. The user has to know about `roco interact --prompt` or `roco story` to get value.

#### 2. No `roco session new` — only `roco session create`
The convention in most CLI tools is `new`, not `create`. `roco session new` feels more natural.

#### 3. No `roco workspace new` command
There's no explicit workspace creation command. Workspaces are created implicitly when running `roco story`. A common user wouldn't know this exists.

#### 4. Session workflow is unclear
After creating a session, the user doesn't know what to do next. The help text doesn't show the progression:
```
roco session create → roco workspace new → roco session <id> -p "Use workspace..." → roco session <id> -p "Write story about X"
```

#### 5. No `roco` one-shot story mode
The main entry point `roco` starts an interactive chat, not a one-shot story generator. A user wanting to quickly generate a story has to know to use `roco story`.

#### 6. Help text is technical
The help text shows all subcommands but doesn't prioritize the common workflows. A new user doesn't know which command to use for which task.

#### 7. No quickstart guide
New users don't know:
- How to install
- What model they need
- Whether they can run without GPU
- Where stories go

## Recommendations

### High Priority
1. **Add `roco -p "..."` shortcut** — Route to either interact or story based on context
2. **Add `roco session new` alias** — Make `create` and `new` synonyms
3. **Add `roco workspace new`** — Explicit workspace creation
4. **Update help text** — Show common workflows first
5. **Add `roco quickstart`** — First-run guide

### Medium Priority
6. **Show session workflow in help** — Document the session → workspace → chat flow
7. **Add progress indicators** — Users wonder if app is stuck during long waits
8. **Improve error messages** — Add actionable hints

### Low Priority
9. **Show output path prominently** — Currently buried in output
10. **Offer story preview** — After publishing

## User Score: 7/10
The magic is real but the entry points are confusing. A user trying `roco -p "make a story"` would be frustrated.
