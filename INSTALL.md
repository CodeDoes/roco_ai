# Installation Guide

How to install and set up RoCo AI.

## Prerequisites

### Required
- **Rust** (1.70 or later) - https://rustup.rs
- **RWKV Model** - Download a RWKV-7 model (2.9B recommended)

### Optional
- **Node.js** (18 or later) - For web editor and plugins
- **Vulkan SDK** - For GPU acceleration

## Quick Install

### 1. Clone the repository
```bash
git clone https://github.com/CodeDoes/roco_ai.git
cd roco_ai
```

### 2. Download a model
```bash
# Create models directory
mkdir -p models

# Download RWKV-7 2.9B model (example)
# Replace with actual model URL
wget -O models/rwkv7-2.9b.st https://example.com/rwkv7-2.9b.st
```

### 3. Build the project
```bash
cargo build --release
```

### 4. Run RoCo
```bash
# Natural language chat (default)
cargo run --release --bin roco -p roco-cli

# Or with a starting prompt
cargo run --release --bin roco -p roco-cli "write a story about a fallen knight"

# Model auto-detects from models/ directory, or set config:
#   .roco/config.toml  →  [model] path = "..."
#   or set RWKV_MODEL env var
```

## Detailed Installation

### Installing Rust

```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add to PATH
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version
```

### Installing Vulkan SDK (for GPU acceleration)

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install vulkan-tools libvulkan-dev
```

**macOS:**
```bash
# Vulkan is included with MoltenVK
brew install molten-vk
```

**Windows:**
Download from https://vulkan.lunarg.com/sdk/home

### Installing Node.js (for web editor and plugins)

```bash
# Using nvm (recommended)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
nvm install 18
nvm use 18

# Or using package manager
# Ubuntu/Debian
sudo apt install nodejs npm

# macOS
brew install node

# Windows
# Download from https://nodejs.org
```

## Setting Up the Web Editor

### 1. Install dependencies
```bash
cd apps/editor
npm install
```

### 2. Start the development server
```bash
npm run dev
```

### 3. Open in browser
Open http://localhost:5173

## Setting Up the API Server

### 1. Build the server
```bash
cargo build --release -p roco-server
```

### 2. Run the server
```bash
# Model auto-detected or read from config
cargo run --release --bin roco -p roco-cli server
```

The server will start at http://localhost:3000

## Setting Up Editor Plugins

### Obsidian

1. Copy `apps/plugins/obsidian/` to your vault's `.obsidian/plugins/roco-ai/`
2. Enable the plugin in Obsidian settings
3. Configure API URL in plugin settings (default: http://localhost:3000)

### VSCode

1. Open `apps/plugins/vscode/` in VSCode
2. Press F5 to launch Extension Development Host
3. Or package with `vsce package` and install

### Zed

1. Build the extension:
```bash
cd apps/plugins/zed
cargo build --release
```
2. Copy the built library to Zed's extensions directory
3. Enable in Zed settings

## Configuration

### Config File (Recommended)

Create a `config.toml` in `.roco/` (next to this project):
```toml
[model]
path = "models/rwkv7-2.9b.st"
vocab = "assets/vocab/rwkv_vocab_v20230424.json"

[server]
host = "127.0.0.1"
port = 8080

[gateway]
host = "127.0.0.1"
port = 8000
rate_limit = 60
```

Config search order (first found wins):
1. `$ROCO_CONFIG` — explicit config file path
2. `.roco/config.toml` in current directory
3. `~/.config/roco/config.toml`
4. `~/.roco/config.toml`

### Environment Variables

Environment variables always beat config file values.

| Variable | Description | Default |
|----------|-------------|---------|
| `RWKV_MODEL` | Path to model file | Auto-detect from `models/` |
| `RWKV_QUANT` | Quantization mode | Auto (NF4 for large models) |
| `RWKV_ADAPTER` | GPU adapter | First Vulkan adapter |
| `RWKV_VOCAB` | Vocabulary file | Auto-detect from `assets/` |

### Model Placement

Place your model files in the `models/` directory:
```
models/
├── rwkv7-2.9b.st
├── rwkv7-1.5b.st
└── rwkv7-0.1b.st
```

The system will auto-detect the model to use based on available VRAM. If no config and no env var is set, it scans `models/` for `rwkv7*.st` files.

## Troubleshooting

### "No model found"
- Make sure you have a `.st` file in the `models/` directory
- Or set `RWKV_MODEL` environment variable

### "Vulkan not found"
- Install Vulkan SDK for your platform
- Or set `RWKV_ADAPTER=llvmpipe` for CPU fallback

### "Out of memory"
- Use a smaller model (1.5B or 0.1B)
- Or set `RWKV_QUANT=nf4` for quantization

### "Slow generation"
- Build with `--release` flag
- Enable GPU acceleration with Vulkan
- Use quantization: `RWKV_QUANT=nf4`

### "API server not starting"
- Make sure port 3000 is available
- Check if another process is using the port
- Try a different port: `PORT=3001 cargo run --release -p roco-server`

## Next Steps

1. **Write your first story**: See [Quick Start Guide](QUICKSTART.md)
2. **Configure the editor**: See [Editor Guide](EDITOR.md)
3. **Learn the commands**: See [Command Reference](COMMANDS.md)
4. **Join the community**: See [Community](COMMUNITY.md)
