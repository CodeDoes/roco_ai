#!/usr/bin/env bash
# === restart_dev_daemons.sh — Manual restart for dev daemons ===
#
# Kills any running inferd + gateway and re-starts them via `cargo run`.
# Used by `cargo watch` in the old watch mode, or standalone for manual restart.
#
# Usage:
#   ./scripts/restart_dev_daemons.sh

set -euo pipefail

ROCO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROCO_DIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RESET='\033[0m'

ROCO_PID_DIR="${ROCO_PID_DIR:-/tmp/roco}"
mkdir -p "$ROCO_PID_DIR"

INFERD_PIDFILE="$ROCO_PID_DIR/inferd.pid"
GATEWAY_PIDFILE="$ROCO_PID_DIR/gateway.pid"

echo -e "${BLUE}ℹ${RESET} Restarting dev daemons via cargo run..."

# Kill old processes
if [[ -f "$INFERD_PIDFILE" ]]; then
    OLD=$(cat "$INFERD_PIDFILE" 2>/dev/null || true)
    if [[ -n "$OLD" ]] && kill -0 "$OLD" 2>/dev/null; then
        kill "$OLD" 2>/dev/null || true
        sleep 0.5
    fi
fi
pkill -f "roco-inferd" 2>/dev/null || true
rm -f "$INFERD_PIDFILE" "$ROCO_PID_DIR/server.pid"

if [[ -f "$GATEWAY_PIDFILE" ]]; then
    OLD=$(cat "$GATEWAY_PIDFILE" 2>/dev/null || true)
    if [[ -n "$OLD" ]] && kill -0 "$OLD" 2>/dev/null; then
        kill "$OLD" 2>/dev/null || true
        sleep 0.5
    fi
fi
pkill -f "roco.*gateway" 2>/dev/null || true
rm -f "$GATEWAY_PIDFILE"

# Start inferd via cargo run
echo -e "${BLUE}ℹ${RESET} Starting inferd..."
cargo run -p roco-inferd -- --port 18080 >> "$ROCO_PID_DIR/inferd_18080.log" 2>&1 &
INFERD_PID=$!
echo "$INFERD_PID" > "$INFERD_PIDFILE"
echo -e "${GREEN}✓${RESET} roco-inferd restarted (cargo PID $INFERD_PID)"

# Start gateway via cargo run
echo -e "${BLUE}ℹ${RESET} Starting gateway..."
cargo run --bin roco -- gateway --detach >> "$ROCO_PID_DIR/gateway_18000.log" 2>&1 &
GATEWAY_PID=$!
echo "$GATEWAY_PID" > "$GATEWAY_PIDFILE"
echo -e "${GREEN}✓${RESET} roco gateway restarted (cargo PID $GATEWAY_PID)"

# Brief health check
sleep 1.5
if curl -sf http://127.0.0.1:18080/health > /dev/null 2>&1; then
    echo -e "${GREEN}✓${RESET} inferd healthy: http://127.0.0.1:18080/health"
else
    echo -e "${YELLOW}⚠${RESET} inferd started, initializing model..."
fi
if curl -sf http://127.0.0.1:18000/health > /dev/null 2>&1; then
    echo -e "${GREEN}✓${RESET} gateway healthy: http://127.0.0.1:18000/health"
else
    echo -e "${YELLOW}⚠${RESET} gateway starting..."
fi