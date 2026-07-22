# Command Reference — RoCo AI

> Quick lookup for CLI, web, and desktop commands.

## CLI (`roco` binary / `start.sh`)

### Story Writing

| Command | What it does |
|---|---|
| `start.sh` | Interactive story writer (recommended for beginners) |
| `start.sh "premise"` | Interactive writer with a starting premise |
| `RWKV_MODEL=... cargo run --release --example story_human -p roco-cli` | Direct CLI invocation |
| `RWKV_MODEL=... cargo run --release --example story_engine -p roco-cli --auto` | Auto-mode (runs to completion) |
| `RWKV_MODEL=... cargo run --release --example story_collaborative -p roco-cli` | Conversational variant |
| `RWKV_MODEL=... cargo run --release --example story_full -p roco-cli` | Full settings demo |

### Resume a Story

```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli \
  --resume .roco/workspaces/story_1234567890
```

### Resume with Existing Text

```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli \
  --from-file my_story.md
```

### Subcommands (roco binary)

| Subcommand | Purpose |
|---|---|
| `roco eval` | Run evaluation suite; saves `.snapshot.json` |
| `roco bless` | Update `oracle:` fields with current snapshot |
| `roco rwkv` | Smoke-test RWKV backend |
| `roco grammar` | Grammar-constrained decode smoke test |
| `roco gpu-check` | Show Vulkan device + model status |
| `roco server [--story] [--detach] [--port]` | Run local HTTP API (editor / plugin hosts) |
| `roco gateway [--target URL] [--rate-limit N]` | Run inference gateway (proxy/cache) |
| `coco gui` | Start the desktop GUI (auto-starts gateway) |
| `coco stop` | Stop background inference + gateway |
| `coco story <prompt>` | Run the structured short-story pipeline (outline → wiki → chapters → publication) |
| `coco export <story-dir> [--format md\|html\|txt] [--output PATH]` | Bundle a finished `.roco/workspaces/story_*` directory into one Markdown / HTML / plain-text file |
| `coco interact [--interactive] [--prompt P] [--resume S] [--pace MODE]` | Conversational CLI with pacing control, session resume |

## Web Apps (`apps/`)

| App | Start Command | URL |
|---|---|---|
| Chat (`apps/chat/`) | `npm run dev` (after `npm install`) | `http://localhost:3000` |
| Studio (`apps/studio/`) | `npm run dev` | `http://localhost:3000` (default) |
| Editor (`apps/editor/`) | `npm run dev` | `http://localhost:5173` |

### Web App Prerequisites

```bash
# Start the Rust API server (required for chat/studio)
RWKV_MODEL=... cargo run --release -p roco-server

# In another terminal, start the web UI
cd apps/chat && npm install && npm run dev
```

## Editor Plugins

| Plugin | Path | Setup Command |
|---|---|---|
| VSCode | `apps/plugins/vscode/` | Open folder, `npm install`, press F5 |
| Zed | `apps/plugins/zed/` | Copy to Zed extensions directory |
| Obsidian | `apps/plugins/obsidian/` | Copy to `.obsidian/plugins/roco-ai/` |

### Plugin Commands

- `RoCo: Generate Chapter`
- `RoCo: Continue Writing`
- `RoCo: Check Quality`
- `RoCo: Apply Feedback`

## Desktop (`crates/ui/`)

| Task | Command |
|---|---|
| Build desktop widgets | `cargo build --release -p roco-ui` |
| Run desktop binary (when available) | `cargo run --release -p roco-ui --bin roco-desktop` |
| View desktop docs | `cargo doc -p roco-ui --open` |

> Note: The desktop GUI is the planned primary surface (see `roadmap/README.md`). It uses `egui` for immediate-mode rendering.
