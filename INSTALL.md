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

### 4. Run the story writer
```bash
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release --example story_human -p roco-cli
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
RWKV_MODEL=models/rwkv7-2.9b.st cargo run --release -p roco-server
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

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RWKV_MODEL` | Path to model file | Auto-detect from `models/` |
| `RWKV_QUANT` | Quantization mode | Auto (NF4 for large models) |
| `RWKV_ADAPTER` | GPU adapter | First Vulkan adapter |
| `RWKV_VOCAB` | Vocabulary file | Auto-detect |

### Model Placement

Place your model files in the `models/` directory:
```
models/
├── rwkv7-2.9b.st
├── rwkv7-1.5b.st
└── rwkv7-0.1b.st
```

The system will auto-detect the model to use based on available VRAM.

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
