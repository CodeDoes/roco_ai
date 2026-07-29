#!/bin/bash
# check_mold.sh — Verify mold linker availability and provide fallback instructions
set -e

echo "Checking for mold linker..."

if command -v mold &> /dev/null; then
    echo "✓ mold found: $(which mold)"
    echo "  Version: $(mold --version 2>&1 | head -1)"
else
    echo "✗ mold not found"
    echo ""
    echo "Options:"
    echo "  1. Install mold:  sudo apt install mold"
    echo "  2. Use lld:       Remove '-fuse-ld=mold' from .cargo/config.toml"
    echo "  3. Use default:   Set RUSTFLAGS='' to override linker settings"
    echo ""
    echo "Current .cargo/config.toml:"
    cat .cargo/config.toml 2>/dev/null | grep -A 5 "rustflags" || echo "  (no rustflags set)"
fi
