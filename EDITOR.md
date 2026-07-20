# Editor Guide — Web Editor (`apps/editor/`)

> The web editor is a simple Vite-based markdown editor. It connects to the `roco-server` API.

## Setup

```bash
# 1. Ensure the server is running
RWKV_MODEL=... cargo run --release -p roco-server

# 2. Start the editor
cd apps/editor
npm install
npm run dev

# 3. Open browser
open http://localhost:5173
```

## Features

- Markdown editing with syntax highlighting
- File browser for navigating `.roco/workspaces/`
- Auto-save (to workspace directory)
- Connection to `roco-server` for AI generation

## File Structure in Editor

The editor shows files from your workspace (`.roco/workspaces/<story_id>/`):

| File | Description |
|---|---|
| `01-OUTLINE.md` | Story outline |
| `02-DIRECTION.md` | Tone, style, themes, pacing |
| `03-CHAPTER_1.md` ... | Individual chapters |
| `06-STORY.md` | Complete assembled story |
| `07-PLOT-STATE.json` | Internal plot tracking |
| `08-QUALITY-*.md` | Quality assessment reports |

## Editing Workflow

1. Open `01-OUTLINE.md` to edit the outline.
2. Click "Generate Chapter" to create the next chapter.
3. Edit the generated text directly.
4. Save — changes persist to the workspace directory.

## Common Issues

- **"Server not found"** — Make sure `roco-server` is running on the default port.
- **No model output** — Check `RWKV_MODEL` is set when starting the server.
- **Changes not saved** — Verify you have write permissions to `.roco/workspaces/`.
