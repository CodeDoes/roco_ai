# Runtime Directory Layout (.roco/)

> Canonical layout for `.roco/` — the agent's runtime state root.
> All content is gitignored. This file documents the intended structure.

## Layout

```
.roco/
├── sessions/          ← SessionStore (crates/session/src/store.rs)
│   └── {session_id}/  ← One directory per session
│       ├── session.log            ← Conversation turns
│       ├── trace.txt              ← Raw I/O transcript
│       ├── meta.json              ← Config + parent ref
│       └── history-{branch}.jsonl ← Branch checkpoints
├── workspaces/        ← Agent workspace artifacts
│   └── story_*/       ← Story pipeline output (timestamped)
└── tests/             ← Test output (devenv)
    └── latest.log
```

## Directory Purposes

### sessions/
**Intended for:** `SessionStore` (`crates/session/src/store.rs`) creates structured session directories with conversation logs, traces, metadata, and branch history.

**Currently used by:** Not yet wired into story pipeline or mechanistic agent. When integrated, each session gets its own subdirectory with the structured layout above.

**Note:** Old flat JSON session files (`s*.json`) from a previous chat implementation have been removed. The new session store uses structured directories.

### workspaces/
**Intended for:** Agent workspace artifacts from the story generation pipeline.

**Used by:**
- `crates/cli/examples/story.rs` (grammar-constrained, creates `story_<prompt>_<ts>/` directories)
- `crates/cli/examples/story_pilot.rs` (grammar-constrained, creates `story_<prompt>_<ts>/` directories)

**Structure:** Each story run creates a timestamped directory:
```
workspaces/
└── story_make_me_a_xianxia_1784145415/
    ├── 01-OUTLINE.md
    ├── 02-WIKI.md
    ├── 03-CHAPTER_{1,2,3}.md
    ├── 04-VALIDATION.md
    ├── 05-SYNOPSIS.md
    └── 06-STORY.md
```

### tests/
**Intended for:** Test output redirection from devenv. Test harness writes to `.roco/tests/latest.log` instead of using shell redirects.

**Used by:** Test infrastructure (devenv.nix).

## Architecture Principles

1. **Structured over flat:** Session data uses structured directories with multiple files per session, not flat JSON files.
2. **Timestamped for uniqueness:** Workspace directories include timestamps to prevent collision across repeated runs.
3. **Code owns the structure:** Each directory is created by specific code paths. If a directory exists but no code creates it, it's orphaned and should be removed.

## Cleanup History (2025-07-15)

The following orphaned directories were removed:
- `.roco/story/` — No current code creates this. Orphaned from old story pipeline.
- `.roco/logs/` — No current code creates this. Orphaned from old tracing setup.
- `.roco/traces/` — No current code creates this. Orphaned from old trace viz experiments.
- `.roco/sessions/*.json` — Old flat JSON files from previous chat implementation.
- `.roco/workspaces/dx-demo-*` — Old demo workspaces.
- `.roco/workspaces/temp-demo` — Old demo workspace.
