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
| `RoCo: Generate Chapter` | Creates the next chapter based on outline |
| `RoCo: Continue Writing` | Extends current text |
| `RoCo: Check Quality` | Runs quality evaluation |
| `RoCo: Apply Feedback` | Applies natural-language feedback |
| `RoCo: Edit Outline` | Opens outline editor |

### Requirements

- `roco-server` running locally (`cargo run --release -p roco-server`)
- `RWKV_MODEL` set in server environment

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
