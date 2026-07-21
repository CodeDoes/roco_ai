# Plugin Guide — VSCode, Zed, Obsidian

> Editor plugins let you generate and edit story content directly inside your favorite editor.

## VSCode (`apps/plugins/vscode/`)

### Setup

```bash
cd apps/plugins/vscode
npm install
# Open the folder in VSCode
code .
# Press F5 to launch Extension Development Host
```

### Commands (Command Palette: `Ctrl+Shift+P` or `Cmd+Shift+P`)

| Command | What it does |
|---|---|
| `RoCo: Generate Chapter` | Creates the next chapter based on outline (requires chapter number) |
| `RoCo: Continue Writing` | Extends current text from cursor position |
| `RoCo: Get Suggestions` | Fetches AI suggestions for current document |
| `RoCo: Check Quality` | Runs quality evaluation for a chapter (opens webview report) |
| `RoCo: Revise Selection` | Revises selected text based on natural-language feedback |
| `RoCo: Add Comment` | Inserts an inline comment annotation |
| `RoCo: Show Plot State` | Opens a webview showing characters, locations, conflicts |

### Requirements

- `roco-server` running with `--story` (`roco server --story --detach`)
- `RWKV_MODEL` set in server environment
- Default API URL: `http://localhost:8080` (configurable via VSCode settings → `roco.apiUrl`)

## Connection Notes

All plugins communicate with the RoCo server over HTTP. Story-specific endpoints (chapter
generate, revise, quality, plot state) require the server to be started with the `--story`
flag:

```bash
roco server --story --detach
```

Without `--story`, only the base endpoints (`/health`, `/complete`, `/v1/completions`, `/vocab`)
are available.

## Zed (`apps/plugins/zed/`)

### Setup

```bash
# Copy extension to Zed extensions directory
mkdir -p ~/.config/zed/extensions
cp -r apps/plugins/zed/* ~/.config/zed/extensions/roco-ai/
# Restart Zed
```

### Features

- In-editor chapter generation
- Quality checking
- Outline editing via Zed's text editor

## Obsidian (`apps/plugins/obsidian/`)

### Setup

```bash
# Copy plugin folder
mkdir -p ~/.obsidian/plugins/roco-ai
cp -r apps/plugins/obsidian/* ~/.obsidian/plugins/roco-ai/
# Enable in Obsidian settings → Community plugins
```

### Commands (Obsidian Command Palette)

- `RoCo: Generate Chapter`
- `RoCo: Continue Writing`
- `RoCo: Check Quality`

### Note

Obsidian plugins are TypeScript-based. They communicate with the `roco-server` HTTP API (`http://localhost:8080` by default).
