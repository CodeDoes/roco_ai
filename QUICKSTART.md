# Quick Start Guide

Get started with RoCo AI in 5 minutes.

## Step 1: Install

```bash
# Clone the repo
git clone https://github.com/CodeDoes/roco_ai.git
cd roco_ai

# Build
cargo build --release
```

## Step 2: Get a Model

Download a RWKV-7 model:
```bash
mkdir -p models
# Download RWKV-7 2.9B (recommended)
# Replace with actual URL
wget -O models/rwkv7-2.9b.st https://example.com/rwkv7-2.9b.st
```

## Step 3: Write Your First Story

```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli
```

The tool will guide you through:
1. Setting the tone and style
2. Creating an outline
3. Writing chapters
4. Giving feedback
5. Publishing the story

## Step 4: Try Different Modes

### Interactive Mode (recommended for beginners)
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli
```
You'll see each chapter and can give feedback.

### Automatic Mode (for experienced users)
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_engine -p roco-cli \
  "Write a dark fantasy about a fallen knight"
```
The agent runs to completion without stopping.

### With a Premise
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli \
  "Write a xianxia story about a lone cultivator who levels up alone"
```

## Step 5: Use the Web Editor (Optional)

```bash
# Start the API server
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release -p roco-server

# In another terminal, start the web editor
cd apps/editor
npm install
npm run dev
```

Open http://localhost:5173 in your browser.

## Step 6: Use Editor Plugins (Optional)

### Obsidian
1. Copy `apps/plugins/obsidian/` to `.obsidian/plugins/roco-ai/`
2. Enable the plugin in Obsidian settings
3. Use commands: `RoCo: Generate Chapter`, `RoCo: Continue Writing`, etc.

### VSCode
1. Open `apps/plugins/vscode/` in VSCode
2. Press F5 to launch Extension Development Host
3. Use commands: `RoCo: Generate Chapter`, `RoCo: Continue Writing`, etc.

## What You Can Do

### Write a Story
- Set the tone (dark, light, humorous)
- Set the style (literary, pulp, minimalist)
- Set themes (redemption, revenge, love)
- Create and edit the outline
- Write chapters one at a time
- Give feedback in plain English
- Publish the finished story

### Give Feedback
You can give feedback in plain English:
- "make it darker"
- "add more dialogue"
- "the pacing is too slow"
- "I want the knight to hesitate"

### Edit the Outline
You can edit the outline:
- "add 2 The Knight's Past: A flashback to the knight's childhood"
- "remove 3"
- "move 1 to 3"

### Get Suggestions
The AI can suggest:
- Continuations of your text
- Alternative approaches
- Fill-in-the-middle text
- Improvements to existing text

## Tips

### Start Simple
Don't try to write a novel on your first try. Start with a short story (3-5 chapters).

### Use the Outline
The outline helps the AI understand your story. Spend time making it good.

### Give Feedback
The AI learns from your feedback. The more you give, the better it gets.

### Be Specific
Instead of "make it better", say "add more dialogue" or "make the pacing faster".

### Use the Web Editor
The web editor is easier to use than the CLI. Try it if you can.

## Common Issues

### "No model found"
Make sure you have a `.st` file in the `models/` directory.

### "Slow generation"
Build with `--release` flag and enable GPU acceleration.

### "Out of memory"
Use a smaller model or enable quantization.

## Next Steps

- [Installation Guide](INSTALL.md) - Detailed installation instructions
- [Command Reference](COMMANDS.md) - All available commands
- [Editor Guide](EDITOR.md) - How to use the web editor
- [Plugin Guide](PLUGINS.md) - How to use editor plugins
- [API Reference](API.md) - API documentation
