#!/bin/bash
# run_desktop.sh — Launch the desktop GUI (egui)
# Usage: ./run_desktop.sh
#
# The desktop GUI is built with egui and wired through `roco gui`.
# It requires the `desktop` feature flag.

set -euo pipefail

echo "========================================"
echo "  RoCo AI — Desktop GUI"
echo "========================================"
echo ""

# Check if we have a model configured
if [ -z "${RWKV_MODEL:-}" ] && [ ! -f .roco/config.toml ]; then
    echo "⚠ No model configured. The GUI will start but you won't be able to generate."
    echo "  Set RWKV_MODEL or create .roco/config.toml"
    echo ""
fi

echo "Starting desktop GUI..."
echo "  Build: cargo run --features desktop -p roco-cli -- gui"
echo ""

cargo run --features desktop -p roco-cli -- gui
