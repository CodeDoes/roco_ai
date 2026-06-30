# Skills System

Modular capability extensions, inspired by OpenClaw's skills but simpler.

## Skill Structure

```
skills/
├── file-ops/            # File read/write/edit
│   ├── manifest.json    # name, description, tools, dependencies
│   ├── tools/
│   │   ├── read.ts
│   │   └── write.ts
│   └── README.md
├── git/                 # Git operations
├── web/                 # Web browsing
└── calendar/            # Calendar management
```

## Manifest

```json
{
  "name": "file-ops",
  "description": "Read, write, and edit files",
  "version": "1.0.0",
  "tools": ["read", "write", "edit"],
  "dependencies": [],
  "requires": ["filesystem"]
}
```

## Agent Integration

Skills register tools with the agent loop. Agent detects available skills from manifest, presents tool descriptions to model, routes tool calls to appropriate skill handler.

## Dynamic Loading

- Skills are directories, not code bundles
- AI can generate new skills at runtime (create dir + manifest + tool code)
- Skills can be enabled/disabled per session
- State-tuned states can be packaged as skills (prose skill = prose mode state)
