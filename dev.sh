#!/usr/bin/env bash
# === dev.sh — RoCo AI development environment ===
#
# Starts the inference daemon and gateway, then watches for code changes
# with auto-reload. The daemons stay alive across code reloads.
#
# Usage:
#   ./dev.sh              → Start daemons + watch all crates
#   ./dev.sh pet          → Start daemons + watch + launch desktop pet
#   ./dev.sh --no-watch   → Just start daemons, no hot reload
#   ./dev.sh --stop       → Stop all daemons
#
# Hot-reload uses `cargo watch` (installed via Nix).
# Daemons run in background and are managed via PID files.

set -euo pipefail

ROCO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$ROCO_DIR"

# ── Colors ──────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
DIM='\033[2m'
BOLD='\033[1m'
RESET='\033[0m'

info()  { echo -e "${BLUE}ℹ${RESET} $1"; }
ok()    { echo -e "${GREEN}✓${RESET} $1"; }
warn()  { echo -e "${YELLOW}⚠${RESET} $1"; }
err()   { echo -e "${RED}✗${RESET} $1"; }
header(){ echo -e "\n${BOLD}${CYAN}── $1 ──${RESET}"; }

# ── Help ────────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    echo "Usage: ./dev.sh [--no-watch|--stop|pet]"
    echo ""
    echo "  (no args)   Start daemons + watch all crates for changes"
    echo "  pet         Start daemons + watch + launch desktop pet"
    echo "  --no-watch  Just start daemons, exit immediately"
    echo "  --stop      Stop all daemons and exit"
    exit 0
fi

# ── Stop ────────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--stop" ]]; then
    header "Stopping daemons"
    cargo run --bin roco -- stop 2>/dev/null || true
    # Also kill any lingering cargo watch processes
    pkill -f "cargo watch" 2>/dev/null || true
    ok "All stopped."
    exit 0
fi

# ── 1. Ensure inference daemon is built ─────────────────────────────────
header "Checking build"
if ! [[ -f "target/debug/roco-inferd" ]]; then
    info "Building inference daemon (first build may take a while)..."
    cargo build -p roco-inferd 2>&1 | tail -3
    ok "roco-inferd built."
fi

if ! [[ -f "target/debug/roco" ]]; then
    info "Building CLI with desktop feature..."
    cargo build --features desktop 2>&1 | tail -3
    ok "roco (desktop) built."
fi

# ── 2. Start inference daemon ────────────────────────────────────────────
header "Starting daemons"

# Check if already running
INFERD_PIDFILE="/tmp/roco/inferd.pid"
GATEWAY_PIDFILE="/tmp/roco/gateway.pid"

if [[ -f "$INFERD_PIDFILE" ]]; then
    PID=$(cat "$INFERD_PIDFILE")
    if kill -0 "$PID" 2>/dev/null; then
        ok "Inference daemon already running (PID $PID)"
    else
        warn "Stale PID file, starting inference daemon..."
        ./target/debug/roco server &
        INFERD_PID=$!
        echo "$INFERD_PID" > "$INFERD_PIDFILE"
        ok "Inference daemon started (PID $INFERD_PID)"
    fi
else
    info "Starting inference daemon (roco-inferd)..."
    ./target/debug/roco-inferd --port 8080 &
    INFERD_PID=$!
    echo "$INFERD_PID" > "$INFERD_PIDFILE"
    ok "Inference daemon started (PID $INFERD_PID)"
fi

# ── 3. Start gateway ────────────────────────────────────────────────────
if [[ -f "$GATEWAY_PIDFILE" ]]; then
    PID=$(cat "$GATEWAY_PIDFILE")
    if kill -0 "$PID" 2>/dev/null; then
        ok "Gateway already running (PID $PID)"
    else
        warn "Stale PID file, starting gateway..."
        ./target/debug/roco gateway --detach 2>/dev/null || \
            warn "Gateway start may have failed (check logs)"
    fi
else
    info "Starting gateway..."
    ./target/debug/roco gateway --detach 2>/dev/null || \
        warn "Gateway start may have failed (check logs)"
fi

# Wait for gateways to become healthy
sleep 2

# ── 4. Verify daemons are up ────────────────────────────────────────────
header "Health check"
HEALTHY=true

if curl -sf http://127.0.0.1:8080/health > /dev/null 2>&1; then
    ok "Inference server:  http://127.0.0.1:8080/health"
else
    warn "Inference server not healthy yet (may still be loading model)"
fi

if curl -sf http://127.0.0.1:8000/health > /dev/null 2>&1; then
    ok "Gateway:           http://127.0.0.1:8000/health"
else
    warn "Gateway not healthy yet"
fi

# ── 5. Launch pet (optional) ────────────────────────────────────────────
if [[ "${1:-}" == "pet" ]]; then
    header "Launching desktop pet"
    info "Pet window will appear (transparent, always-on-top)."
    ./target/debug/roco pet &
    PET_PID=$!
    ok "Pet launched (PID $PET_PID)"
fi

# ── 6. Hot reload (unless --no-watch) ────────────────────────────────────
if [[ "${1:-}" == "--no-watch" ]]; then
    header "Daemons running. Press Ctrl+C to stop."
    info "Inference: http://127.0.0.1:8080"
    info "Gateway:   http://127.0.0.1:8000"
    info ""
    info "Run ./dev.sh --stop to stop daemons."
    echo ""
    # Wait for Ctrl+C
    trap '' INT
    while true; do sleep 10; done
else
    header "Hot reload enabled — watching for changes"
    info "Any code change rebuilds and restarts the CLI."
    info "Daemons (inference + gateway) stay alive."
    info ""
    info "Watching: crates/cli/ crates/app/ crates/ui/ crates/agent/"
    info "Press Ctrl+C to stop."
    echo ""

    # cargo watch on relevant crates
    cargo watch \
        --watch crates/cli/ \
        --watch crates/app/ \
        --watch crates/ui/ \
        --watch crates/agent/ \
        --watch crates/grammar/ \
        --watch crates/engine/ \
        --watch crates/message/ \
        --watch crates/tools/ \
        --watch crates/workspace/ \
        --watch crates/session/ \
        --shell "cargo check --features desktop 2>&1 | tail -5 && echo '${GREEN}✓ Build OK${RESET}' || echo '${RED}✗ Build failed${RESET}'"
fi
