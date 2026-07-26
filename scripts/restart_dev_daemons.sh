#!/usr/bin/env bash
set -euo pipefail

ROCO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROCO_DIR"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RESET='\033[0m'

ROCO_PID_DIR="${ROCO_PID_DIR:-/tmp/roco}"
INFERD_PIDFILE="$ROCO_PID_DIR/inferd.pid"
GATEWAY_PIDFILE="$ROCO_PID_DIR/gateway.pid"

echo -e "${BLUE}ℹ${RESET} Rebuilding binaries (roco-inferd, roco CLI & gateway)..."

if cargo build --release -p roco-inferd 2>&1 && cargo build -p roco-cli --features desktop,net 2>&1; then
    echo -e "${GREEN}✓ Build succeeded.${RESET} Restarting daemons..."

    # Stop running inferd process
    if [[ -f "$INFERD_PIDFILE" ]]; then
        PID=$(cat "$INFERD_PIDFILE" 2>/dev/null || true)
        if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
            kill "$PID" 2>/dev/null || true
            sleep 0.5
        fi
        rm -f "$INFERD_PIDFILE" "$ROCO_PID_DIR/server.pid"
    fi
    pkill -f "roco-inferd" 2>/dev/null || true

    # Stop running gateway process
    if [[ -f "$GATEWAY_PIDFILE" ]]; then
        PID=$(cat "$GATEWAY_PIDFILE" 2>/dev/null || true)
        if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
            kill "$PID" 2>/dev/null || true
            sleep 0.5
        fi
        rm -f "$GATEWAY_PIDFILE"
    fi
    pkill -f "roco gateway" 2>/dev/null || true

    # Start roco-inferd
    mkdir -p "$ROCO_PID_DIR"
    TARGET_DIR="${CARGO_TARGET_DIR:-target}"
    "${TARGET_DIR}/release/roco-inferd" --port 8080 > "$ROCO_PID_DIR/inferd_8080.log" 2>&1 &
    INFERD_PID=$!
    echo "$INFERD_PID" > "$INFERD_PIDFILE"
    echo "$INFERD_PID" > "$ROCO_PID_DIR/server.pid"
    echo -e "${GREEN}✓ roco-inferd restarted (PID $INFERD_PID)${RESET}"

    # Start gateway
    "${TARGET_DIR}/debug/roco" gateway --detach > "$ROCO_PID_DIR/gateway_8000.log" 2>&1 || true
    echo -e "${GREEN}✓ roco gateway restarted.${RESET}"

    # Brief health check
    sleep 1.5
    if curl -sf http://127.0.0.1:8080/health > /dev/null 2>&1; then
        echo -e "${GREEN}✓ inferd healthy: http://127.0.0.1:8080/health${RESET}"
    else
        echo -e "${YELLOW}⚠ inferd started, initializing model...${RESET}"
    fi

    if curl -sf http://127.0.0.1:8000/health > /dev/null 2>&1; then
        echo -e "${GREEN}✓ gateway healthy: http://127.0.0.1:8000/health${RESET}"
    else
        echo -e "${YELLOW}⚠ gateway starting...${RESET}"
    fi
else
    echo -e "${RED}✗ Build failed. Keeping existing daemons running.${RESET}"
fi