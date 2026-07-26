#!/usr/bin/env bash
# === dev.sh — RoCo AI development environment ===
#
# Starts the inference daemon and gateway via `cargo run` so rebuilds happen
# automatically. Optional `cargo watch` mode re-starts on every code change.
#
# Usage:
#   ./dev.sh              → Start daemons (no watch)
#   ./dev.sh --watch      → Start daemons + cargo watch (hot reload)
#   ./dev.sh --no-watch   → Start daemons without watching
#   ./dev.sh pet          → Start daemons + launch desktop pet
#   ./dev.sh --stop       → Stop all daemons

set -euo pipefail

ROCO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$ROCO_DIR"

# PID/log directory — respects $ROCO_PID_DIR, falls back to /tmp/roco
ROCO_PID_DIR="${ROCO_PID_DIR:-/tmp/roco}"
mkdir -p "$ROCO_PID_DIR"

INFERD_PIDFILE="$ROCO_PID_DIR/inferd.pid"
GATEWAY_PIDFILE="$ROCO_PID_DIR/gateway.pid"

cleanup() {
    echo ""
    echo "Shutting down dev daemons..."
    # Kill cargo-watch instances first
    pkill -f "cargo.*watch.*roco" 2>/dev/null || true
    # Kill the daemon binaries (and any cargo parent)
    pkill -f "roco-inferd" 2>/dev/null || true
    pkill -f "roco.*gateway.*--detach" 2>/dev/null || true
    rm -f "$INFERD_PIDFILE" "$GATEWAY_PIDFILE" "$ROCO_PID_DIR/server.pid" 2>/dev/null || true
    echo "Done."
}

# ── Colors ──────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

info()  { echo -e "${BLUE}ℹ${RESET} $1"; }
ok()    { echo -e "${GREEN}✓${RESET} $1"; }
warn()  { echo -e "${YELLOW}⚠${RESET} $1"; }
err()   { echo -e "${RED}✗${RESET} $1"; }
header(){ echo -e "\n${BOLD}${CYAN}── $1 ──${RESET}"; }

# ── Help ────────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    echo "Usage: ./dev.sh [--watch|--no-watch|--stop|pet]"
    echo ""
    echo "  (no args)   Start daemons via cargo run (no watch)"
    echo "  --watch     Start daemons + cargo watch (hot reload)"
    echo "  --no-watch  Start daemons without watching"
    echo "  pet         Start daemons + launch desktop pet"
    echo "  --stop      Stop all daemons and exit"
    exit 0
fi

# ── Stop ────────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--stop" ]]; then
    header "Stopping daemons"
    cleanup
    ok "All stopped."
    exit 0
fi

# ══════════════════════════════════════════════════════════════════════════
# Determine watch mode
# ══════════════════════════════════════════════════════════════════════════
HOTRELOAD="${ROCO_HOTRELOAD:-false}"
if [[ "${1:-}" == "--watch" || "${1:-}" == "--hotreload" || "${1:-}" == "--hot-reload" ]]; then
    HOTRELOAD="true"
fi
if grep -qE "^\s*hotreload\s*=\s*true" .roco/config.toml 2>/dev/null || \
   grep -qE "^\s*hotreload\s*=\s*true" ~/.config/roco/config.toml 2>/dev/null; then
    HOTRELOAD="true"
fi

# ── Watch mode — each daemon runs under cargo watch ─────────────────────
if [[ "$HOTRELOAD" == "true" && "${1:-}" != "--no-watch" ]]; then
    header "Hot reload enabled — watching for changes"
    info "Each daemon runs via cargo watch: rebuild + restart on every code change."
    info "Press Ctrl+C to stop."
    echo ""

    trap cleanup EXIT INT TERM

    # Start inferd under cargo watch
    cargo watch -w crates/ -x "run -p roco-inferd -- --port 18080" \
        > "$ROCO_PID_DIR/inferd_18080.log" 2>&1 &
    echo "$!" > "$INFERD_PIDFILE"

    # Start gateway under cargo watch
    cargo watch -w crates/ -x "run --bin roco -- gateway --detach" \
        > "$ROCO_PID_DIR/gateway_18000.log" 2>&1 &
    echo "$!" > "$GATEWAY_PIDFILE"

    wait
    exit 0
fi

# ══════════════════════════════════════════════════════════════════════════
# No-watch mode — simple background via cargo run
# ══════════════════════════════════════════════════════════════════════════
header "Starting daemons (no watch)"

# ── Inferd ──────────────────────────────────────────────────────────────
if [[ -f "$INFERD_PIDFILE" ]]; then
    OLD_PID=$(cat "$INFERD_PIDFILE")
    if kill -0 "$OLD_PID" 2>/dev/null; then
        ok "Inference daemon already running (PID $OLD_PID)"
    else
        warn "Stale PID file, starting inferd..."
        cargo run -p roco-inferd -- --port 18080 >> "$ROCO_PID_DIR/inferd_18080.log" 2>&1 &
        CARGO_PID=$!
        echo "$CARGO_PID" > "$INFERD_PIDFILE"
        ok "Inferd starting via cargo run (cargo PID $CARGO_PID)"
    fi
else
    info "Starting inference daemon via cargo run..."
    cargo run -p roco-inferd -- --port 18080 >> "$ROCO_PID_DIR/inferd_18080.log" 2>&1 &
    CARGO_PID=$!
    echo "$CARGO_PID" > "$INFERD_PIDFILE"
    ok "Inferd starting via cargo run (cargo PID $CARGO_PID)"
fi

# ── Gateway ─────────────────────────────────────────────────────────────
if [[ -f "$GATEWAY_PIDFILE" ]]; then
    OLD_PID=$(cat "$GATEWAY_PIDFILE")
    if kill -0 "$OLD_PID" 2>/dev/null; then
        ok "Gateway already running (PID $OLD_PID)"
    else
        warn "Stale PID file, starting gateway..."
        cargo run --bin roco -- gateway --detach >> "$ROCO_PID_DIR/gateway_18000.log" 2>&1 &
        CARGO_PID=$!
        echo "$CARGO_PID" > "$GATEWAY_PIDFILE"
    fi
else
    info "Starting gateway via cargo run..."
    cargo run --bin roco -- gateway --detach >> "$ROCO_PID_DIR/gateway_18000.log" 2>&1 &
    CARGO_PID=$!
    echo "$CARGO_PID" > "$GATEWAY_PIDFILE"
fi

sleep 2

# ── Health check ────────────────────────────────────────────────────────
header "Health check"
if curl -sf http://127.0.0.1:18080/health > /dev/null 2>&1; then
    ok "Inference server:  http://127.0.0.1:18080/health"
else
    warn "Inference server not healthy yet (may still be loading model)"
fi
if curl -sf http://127.0.0.1:18000/health > /dev/null 2>&1; then
    ok "Gateway:           http://127.0.0.1:18000/health"
else
    warn "Gateway not healthy yet"
fi

# ── Pet (optional) ──────────────────────────────────────────────────────
if [[ "${1:-}" == "pet" ]]; then
    header "Launching desktop pet"
    info "Pet window will appear (transparent, always-on-top)."
    cargo run --bin roco -- pet >> "$ROCO_PID_DIR/pet.log" 2>&1 &
    PET_PID=$!
    ok "Pet launched via cargo run (cargo PID $PET_PID)"
fi

# ── Idle ────────────────────────────────────────────────────────────────
header "Daemons running. Press Ctrl+C to stop."
info "Inference: http://127.0.0.1:18080"
info "Gateway:   http://127.0.0.1:18000"
echo ""
trap cleanup EXIT INT TERM
while true; do sleep 10; done