# RoCo AI

A collaborative story writing tool where humans and AI work together to create stories.

## Quick Start

```bash
# Start writing (interactive mode)
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli

# Start with a premise
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli \
  "Write a dark fantasy about a fallen knight"
```

That's it. The tool will guide you through the process.

## What You Can Do

### Write a Story from Scratch
```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli
```
The tool will:
1. Ask what kind of story you want
2. Set the tone, style, themes
3. Generate an outline
4. Let you edit the outline
5. Write chapters one at a time
6. Let you give feedback
7. Publish the finished story

### Resume a Story
```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli \
  --resume .roco/workspaces/story_1234567890
```

### Use Existing Text
```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli \
  --from-file my_story.md
```

## How It Works

### 1. Set the Direction
Tell the AI what you want:
- Tone: dark, light, humorous, serious
- Style: literary, pulp, minimalist
- Themes: redemption, revenge, love
- Pacing: fast, slow, building

### 2. Create the Outline
The AI generates an outline. You can:
- Add chapters
- Remove chapters
- Move chapters
- Modify chapters
- Skip and use as-is

### 3. Write Chapters
The AI writes one chapter at a time. You can:
- Accept and continue
- Give feedback ("make it darker", "add more dialogue")
- Ask for quality check
- Skip to next chapter
- Stop and publish

### 4. Give Feedback
You can give feedback in plain English:
- "make it darker"
- "add more dialogue"
- "the pacing is too slow"
- "I want the knight to hesitate"

### 5. Publish
The story is saved to a workspace:
- `06-STORY.md` — the complete story
- `01-OUTLINE.md` — the outline
- `03-CHAPTER_*.md` — individual chapters
- `07-PLOT-STATE.json` — plot state

## Output

Stories are saved to `.roco/workspaces/story_<timestamp>/`:
- `06-STORY.md` — complete story
- `01-OUTLINE.md` — outline
- `03-CHAPTER_1.md`, `03-CHAPTER_2.md`, ... — chapters
- `07-PLOT-STATE.json` — plot state
- `08-QUALITY-*.md` — quality reports

## Examples

### Example 1: Casual Writer
```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli
# Follow the prompts
```

### Example 2: Experienced Writer
```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli
# Set direction: dark, literary, redemption
# Edit outline: add/remove chapters
# Give detailed feedback on each chapter
```

### Example 3: Pantser
```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli
# Set direction: whatever feels right
# Skip outline editing
# See where the story goes
# Give feedback when inspired
```

### Example 4: Plotter
```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli
# Set direction: specific
# Edit outline: detailed planning
# Generate chapters that follow the outline
```

### Example 5: Editor
```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli \
  --from-file existing_story.md
# AI analyzes existing text
# AI continues from where you left off
# Give feedback on continuations
```

### Example 6: Collaborator
```bash
RWKV_MODEL=... cargo run --release --example story_human -p roco-cli
# Write some text
# AI continues
# Write more text
# AI continues
# Back and forth
```

## Features

### Core Engine
- Dynamic outline expansion
- Plot state tracking
- Context assembly
- Chapter continuation
- Quality evaluation
- Revision support
- Session persistence

### Human-AI Interaction
- Interactive mode (human sees each chapter)
- Automatic mode (agent runs to completion)
- Natural language feedback
- Outline editing
- Story direction
- Chapter steering

### Observability
- Model call recording
- Decision tracing
- Action logging
- Quality assessment

### Reversibility
- Workspace snapshots
- Action history
- Undo/redo
- Rollback

## Environment Variables

| Variable | Effect | Default |
|---|---|---|
| `RWKV_MODEL` | Path to `.st` SafeTensors file | First `rwkv7-*.st` in `models/` |
| `RWKV_QUANT` | Override quantization | Auto-picked |
| `RWKV_ADAPTER` | GPU adapter name substring | First Vulkan adapter |

## Building

```bash
cargo build --release
```

## License

See LICENSE file.
