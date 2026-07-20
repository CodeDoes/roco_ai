#!/bin/bash
# start.sh — Quick start for end users
# Usage: ./start.sh ["premise"]

set -euo pipefail

echo "========================================"
echo "  RoCo AI — Start Writing a Story"
echo "========================================"
echo ""

# Check for model
if [ -z "${RWKV_MODEL:-}" ]; then
    MODEL_PATH="models/"$(ls models/*.st 2>/dev/null | head -n 1 || echo "")
    if [ -n "$MODEL_PATH" ]; then
        export RWKV_MODEL="$MODEL_PATH"
        echo "✅ Found model: $MODEL_PATH"
    else
        echo "❌ No RWKV_MODEL set and no .st file in models/"
        echo ""
        echo "To fix:"
        echo "  1. Download a RWKV-7 model (.st file)"
        echo "  2. Place it in the models/ directory"
        echo "  3. Set RWKV_MODEL=path/to/model.st"
        echo ""
        echo "See INSTALL.md for download links."
        exit 1
    fi
fi

echo "✅ RWKV_MODEL=$RWKV_MODEL"
echo ""

# Determine premise
PREMISE="${1:-}"
if [ -n "$PREMISE" ]; then
    echo "📖 Premise: $PREMISE"
    echo ""
fi

echo "Starting interactive story writer..."
echo "Tips:"
echo "  - Press Enter to skip any setup question"
echo "  - Type 'f' during chapters to give feedback"
echo "  - Type 'q' to stop and publish"
echo "  - Your story saves to .roco/workspaces/"
echo ""

if [ -n "$PREMISE" ]; then
    cargo run --release --example story_human -p roco-cli -- "$PREMISE"
else
    cargo run --release --example story_human -p roco-cli
fi
