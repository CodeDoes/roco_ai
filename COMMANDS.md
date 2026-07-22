# Command Reference — RoCo AI

> Quick lookup for CLI, web, and desktop commands.

## Quick Start

```bash
# Natural language mode (default) — just start chatting
roco
roco "write a story about a lighthouse keeper"

# Or use the helper script
./start.sh
./start.sh "a detective in space"

# Model path: auto-detected in models/ directory, or set in config:
#   .roco/config.toml    →   [model] path = "..."
#   $ROCO_CONFIG         →   path to config file
#   $RWKV_MODEL          →   env var (overrides config)
```

## CLI (`roco` binary)

### Default Mode — Natural Language Chat

Running `roco` without a subcommand starts **interactive chat mode** — just type naturally, get AI responses, change pacing, resume sessions.

| Command | What it does |
|---|---|
| `roco` | Start interactive chat (natural language, the default) |
| `roco <prompt>` | Chat with a starting prompt |
| `roco interact [--pace MODE]` | Same as default, with explicit pacing control |
| `roco interact --resume <session>` | Resume a previous session |
| `roco interact --list-sessions` | List saved sessions |

### Structured Pipeline (Optional)

The `story` subcommand runs a formal pipeline (outline → wiki → chapters → validation → synopsis → publish). Use it when you want a structured short story.

| Command | Purpose |
|---|---|
| `roco story <prompt> [--strategy S] [--max-tokens T]` | Generate a structured short story |
| `roco export <story-dir> [--format md\|html\|txt] [--output PATH]` | Bundle a finished workspace into one file |

### Other Subcommands

| Subcommand | Purpose |
|---|---|
| `roco gui` | Start the desktop GUI (auto-starts gateway) |
| `roco server [--story] [--detach] [--port]` | Run local HTTP API (editor / plugin hosts) |
| `roco gateway [--target URL] [--rate-limit N]` | Run inference gateway (proxy/cache) |
| `roco stop` | Stop background inference + gateway |
| `roco eval` | Run evaluation suite; saves `.snapshot.json` |
| `roco bless` | Update `oracle:` fields with current snapshot |
| `roco rwkv` | Smoke-test RWKV backend |
| `roco grammar` | Grammar-constrained decode smoke test |
| `roco gpu-check` | Show Vulkan device + model status |

## Examples (Development)

```bash
# Run the human-centered story writing example
cargo run --release --example story_human -p roco-inference

# Run the story engine example (auto-mode)
cargo run --release --example story_engine -p roco-inference --auto
```

## Web Apps (`apps/`)

| App | Start Command | URL |
|---|---|---|
| Chat (`apps/chat/`) | `npm run dev` (after `npm install`) | `http://localhost:3000` |
| Studio (`apps/studio/`) | `npm run dev` | `http://localhost:3000` (default) |
| Editor (`apps/editor/`) | `npm run dev` | `http://localhost:5173` |

### Web App Prerequisites

```bash
# Start the Rust API server (required for chat/studio)
roco server

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

> Note: The desktop GUI is the planned primary surface (see `roadmap/README.md`). It uses `egui` for immediate-mode rendering. Model path is read from config or `RWKV_MODEL` env var.
