# RoCo AI — Getting Started

> For detailed documentation, see [AGENTS.md](../AGENTS.md).

## What is RoCo?

RoCo AI is an **AI-assisted collaborative writing tool** powered by the RWKV-7 State Space Model. It helps you write stories, run adventure games, create TTRPG campaigns, and more — all with a persistent conversation memory.

## Quick Start

### 1. Tell a Story (Simplest)

```bash
# One-shot story from a premise
roco "A lighthouse keeper discovers a hidden message in the fog"

# Interactive story with pacing control
roco story "A lighthouse keeper discovers a hidden message in the fog"
```

### 2. Chat Naturally

```bash
# Start chatting immediately
roco

# Chat with a starting prompt
roco -p "Help me brainstorm ideas for a sci-fi short story"
```

### 3. Run an Adventure

```bash
roco game
# Or describe what you want:
roco "let's play an adventure set in ancient Rome"
```

### 4. Create TTRPG Campaigns

```bash
roco ttrpg
```

## Requirements

### GPU Required

RoCo uses a **GPU with Vulkan support** for inference. Without a GPU, you can still:
- Use `roco --mock` to run with a simulated backend (for testing)
- Chat in text-only mode (limited functionality)

### Model Setup

1. Download the RWKV-7 model (2.9B parameters)
2. Place it in `models/` or set `RWKV_MODEL` environment variable
3. Run `roco gpu-check` to verify GPU detection

### Ports

Default ports (configurable in `.roco/config.toml`):
- **Inference daemon**: 18080
- **Gateway API**: 18081

## Common Workflows

### Structured Story Pipeline

```bash
# Generate a complete story
roco story "Your premise here"

# Resume an interrupted story
roco story --resume

# Fix a specific chapter
roco story "Your premise" --fix chapter 3
```

### Session Management

```bash
# Create a persistent session
roco session create

# Chat in a session
roco session <session_id> -p "Your message"

# List all sessions
roco session list
```

### Inspect Your Work

```bash
# Story statistics
roco stats

# Show active jobs
roco jobs

# Inspect model/system state
roco inspect
```

## Troubleshooting

### "No Vulkan device found"
- Make sure your GPU drivers are up to date
- Install Vulkan SDK: `sudo apt install libvulkan-dev` (Linux)

### "Model file not found"
- Set the model path: `export RWKV_MODEL=/path/to/model.bin`
- Or place the model in `./models/` in your project directory

### "Port already in use"
- Stop daemons: `roco stop`
- Check what's using the port: `lsof -i :18080`

### Slow generation
- Temperature ≥ 0.7 causes repetition in RWKV-7
- Try `--temperature 0.5` for more creative output

## File Locations

All RoCo data lives in `.roco/` in your current directory:

```
.roco/
├── config.toml          # Configuration
├── workspaces/         # Story workspaces (outline, wiki, chapters)
├── sessions/           # Persistent chat transcripts
└── stories/            # Published stories
```

Override with `$ROCO_DIR` for headless/CI setups.

## Getting Help

```bash
# General help
roco --help

# Specific command help
roco story --help
roco interact --help

# Inspect system
roco inspect
roco gpu-check
```

## Mode Reference

| Mode | Command | Description |
|------|---------|-------------|
| Chat | `roco` or `roco -p` | Natural language conversation |
| Story | `roco story` | Structured 6-phase story pipeline |
| Game | `roco game` | Interactive fiction adventure |
| TTRPG | `roco ttrpg` | Tabletop RPG campaign system |
| World Sim | `roco world-sim` | World building simulation |
| HTML | `roco html` | Live HTML canvas generation |
| Code | `roco code` | AI coding assistant |

## Tips

1. **Start simple**: `roco "your idea"` is the fastest way to get started
2. **Use `--mock`**: For testing without a GPU/model
3. **Resume often**: `--resume` skips completed phases in story pipeline
4. **Session persistence**: Your conversation history is saved automatically
5. **TTY required**: Browser auto-opens only when running interactively

## Advanced

See [AGENTS.md](../AGENTS.md) for:
- Architecture details (§1-8)
- Evaluation methodology (§9)
- Known issues and gotchas (§10)
- Router NLU system (§13)
- Full documentation map (§14)
