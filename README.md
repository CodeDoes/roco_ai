# RoCo AI

> A collaborative story-writing tool where humans and AI work together. The AI writes; you decide.

## Start Here (Choose Your Path)

| I want to... | Go here |
|---|---|
| Write my first story (easiest) | `start.sh` or read [`QUICKSTART.md`](QUICKSTART.md) |
| Understand the project quickly | [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md) |
| Learn the full user journey | [`USER_GUIDE.md`](USER_GUIDE.md) |
| Run tests / verify edits | [`run_tests.sh`](run_tests.sh) |
| Understand what's frozen vs editable | [`EDIT_GUIDE.md`](EDIT_GUIDE.md) |

## Quick Start (30 Seconds)

```bash
# 1. Make sure you have a model file in models/ (see INSTALL.md)
# 2. Run the interactive writer
./start.sh
```

The tool asks a few questions, shows you an outline, and writes chapters one at a time. At each chapter you can:

- **Enter** — continue
- **`f`** + feedback — tell the AI what to change
- **`r`** — check quality
- **`s`** — skip to next chapter
- **`q`** — stop and publish

Your finished story appears in `.roco/workspaces/story_*/` as `06-STORY.md`.

## What You Get (End User)

Every story produces these files:

| File | What's inside |
|---|---|
| `01-OUTLINE.md` | Your story structure |
| `02-DIRECTION.md` | Tone, style, themes, pacing |
| `03-CHAPTER_1.md`, `03-CHAPTER_2.md`, ... | Individual chapters |
| `06-STORY.md` | The complete assembled story |
| `07-PLOT-STATE.json` | Internal plot tracking |
| `08-QUALITY-*.md` | Quality reports (if requested) |

## Two Surfaces

| Surface | Best for | Start it |
|---|---|---|
| **CLI** (`start.sh`) | Beginners, quick writing, scripting | `./start.sh` |
| **Desktop GUI** (`crates/ui/`) | Power users, **primary surface** | See [`run_desktop.sh`](run_desktop.sh) and [`USER_GUIDE.md`](USER_GUIDE.md) |

## Key Documents

- [`QUICKSTART.md`](QUICKSTART.md) — 5-minute setup
- [`INSTALL.md`](INSTALL.md) — Detailed installation (Rust, model download, GPU setup)
- [`USER_GUIDE.md`](USER_GUIDE.md) — Full user journey, giving feedback, common questions
- [`COMMANDS.md`](COMMANDS.md) — CLI, desktop, and plugin commands
- [`API.md`](API.md) — Server endpoints for custom integrations
- [`AGENTS.md`](AGENTS.md) — Full agent behavior philosophy (for developers)
- [`AGENT_GUIDE.md`](AGENT_GUIDE.md) — Short agent rules (for quick reference)
- [`EDIT_GUIDE.md`](EDIT_GUIDE.md) — Which files are frozen / editable
- [`PROJECT_STRUCTURE.md`](PROJECT_STRUCTURE.md) — Directory map and naming explanation

## Examples for Different Writers

### Casual Writer
```bash
./start.sh
# Follow the prompts; press Enter to skip questions
# Accept the outline; see chapters appear one by one
```

### Experienced Writer
```bash
./start.sh "Write a dark fantasy about a fallen knight"
# Set direction: dark, literary, redemption
# Edit outline: add, remove, move chapters
# Give detailed feedback on each chapter
```

### Pantser (Discover as You Go)
```bash
./start.sh
# Skip outline editing; see where the story goes
# Give feedback when inspired
```

### Editor (From Existing Text)
```bash
./start.sh --from-file my_existing_story.md
# AI analyzes existing text and continues
```

### Collaborator (Back-and-Forth)
```bash
RWKV_MODEL=... cargo run --release --example story_collaborative -p roco-cli
# Write some text → AI continues → write more → AI continues
```

## Features

- **Interactive mode** — you see every chapter before the AI continues
- **Automatic mode** — runs to completion (`story_engine`)
- **Natural language feedback** — plain English instructions
- **Outline editing** — add, remove, move, modify chapters
- **Quality evaluation** — check structure, coherence, style
- **Session persistence** — resume any story later
- **Workspace snapshots** — undo / rollback
- **Desktop widgets** — markdown editor with inline AI (in development)

## Environment Variables

| Variable | What it does | Default |
|---|---|---|
| `RWKV_MODEL` | Path to `.st` model file | First `.st` in `models/` |
| `RWKV_QUANT` | Quantization override (`nf4`, `int8`, `none`) | Auto-picked |
| `RWKV_ADAPTER` | GPU adapter substring | First Vulkan adapter |
| `RWKV_GRAMMAR` | GBNF grammar for constrained output | Unset |

> See [`INSTALL.md`](INSTALL.md) for full environment details.

## Building

```bash
# Quick build (debug — may hang with GPU)
cargo build

# Release build (required for GPU work)
cargo build --release

# Verify everything
./run_tests.sh
```

> **Important:** Release builds are required for GPU work. Debug builds hang on most consumer GPUs due to WGPU validation layers (`AGENTS.md`).

## For Developers / Agents

- Read [`AGENT_GUIDE.md`](AGENT_GUIDE.md) before editing any file.
- Read [`EDIT_GUIDE.md`](EDIT_GUIDE.md) to know which files are frozen.
- Read [`roadmap/v1.md`](roadmap/v1.md) for the current focus (experience, not engine).
- The engine (`crates/inference/`, `engine/`, `grammar/`, etc.) is frozen. New work is frontend-only.

## License

See `LICENSE` file.
