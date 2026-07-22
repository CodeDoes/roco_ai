#!/bin/bash
# start.sh — Quick start for end users
# Usage: ./start.sh ["premise"]
#
# No env vars required. Model is auto-detected or read from config:
#   .roco/config.toml  |  $ROCO_CONFIG  |  ~/.config/roco/config.toml

set -euo pipefail

echo "========================================"
echo "  RoCo AI — Collaborative Writing"
echo "========================================"
echo ""
echo "  Natural language mode. Just start writing."
echo "  Model auto-detected or from config file."
echo ""

# Build and run (no env var needed — config handles it)
PREMISE="${1:-}"
if [ -n "$PREMISE" ]; then
    cargo run --release --bin roco -p roco-cli -- "$PREMISE"
else
    cargo run --release --bin roco -p roco-cli
fi
