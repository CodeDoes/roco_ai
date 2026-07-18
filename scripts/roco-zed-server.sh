#!/usr/bin/env bash
# Launcher for the RoCo AI Zed extension backend.
# Run this in a terminal you keep open (do NOT background it here — the
# background process is reaped when the spawning shell exits).
#
#   ./scripts/roco-zed-server.sh
#
# Then in Zed: type /roco <prompt> in the agent panel, or open a Markdown
# file to use the roco language server.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Resolve the model if not already set.
if [[ -z "${RWKV_MODEL:-}" ]]; then
  for cand in \
    "$ROOT/models/"*.st \
    "$ROOT/../models/"*.st \
    /home/kit/Documents/models/*.st ; do
    if [[ -e "$cand" ]]; then
      export RWKV_MODEL="$cand"
      break
    fi
  done
fi

if [[ -z "${RWKV_MODEL:-}" ]]; then
  echo "ERROR: no RWKV model found. Set RWKV_MODEL to a .st SafeTensors file." >&2
  exit 1
fi

# Ensure the binary exists on PATH (symlink into ~/.local/bin).
BIN="$HOME/.local/bin/roco"
if [[ ! -x "$BIN" ]]; then
  TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}/release/roco"
  if [[ -x "$TARGET_DIR" ]]; then
    ln -sf "$TARGET_DIR" "$BIN"
  else
    echo "ERROR: roco binary not built. Run: cargo build --release --bin roco" >&2
    exit 1
  fi
fi

echo "Starting RoCo server for Zed on http://127.0.0.1:8080"
echo "  RWKV_MODEL=$RWKV_MODEL"
echo "  Keep this terminal open. Ctrl-C to stop."
exec "$BIN" server --story
