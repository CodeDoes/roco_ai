#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# roco-stack.sh — manage the local inference stack (inferd + gateway).
#
# Usage:
#   roco-stack.sh up          # ensure both daemons running (returns fast)
#   roco-stack.sh wait        # BLOCK until inferd is healthy (checks process
#                             #   liveness first, then polls HTTP; bounded 10m)
#   roco-stack.sh restart     # kill + restart both
#   roco-stack.sh status      # one-line status
#   roco-stack.sh logs        # tail both logs
#   roco-stack.sh down        # stop both
#
# Env overrides:
#   INFERD_PORT (18080)  GATEWAY_PORT (18000)  ROCO_BIN (./target/release/roco)
#   INFERD_LOG (/tmp/roco-inferd.log)  GATEWAY_LOG (/tmp/roco-gateway.log)
# ─────────────────────────────────────────────────────────────────────────────
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INFERD_PORT="${INFERD_PORT:-18080}"
GATEWAY_PORT="${GATEWAY_PORT:-18000}"
ROCO_BIN="${ROCO_BIN:-$ROOT/target/release/roco}"
INFERD_LOG="${INFERD_LOG:-/tmp/roco-inferd.log}"
GATEWAY_LOG="${GATEWAY_LOG:-/tmp/roco-gateway.log}"

# Process liveness — pgrep -f with full cmdline match.
inferd_pid()  { pgrep -f "roco-inferd --port" | head -1; }
gateway_pid() { pgrep -f "roco gateway --detach" | head -1; }

health() { # $1=port -> "ok"|"degraded"|"down"
  local body
  body=$(curl -s -m 2 "http://127.0.0.1:$1/health" 2>/dev/null) || return 1
  if echo "$body" | grep -qE '"status":"(ok|healthy)"'; then echo ok
  elif [ -n "$body" ]; then echo degraded
  else echo down; fi
}

start_inferd() {
  if [ -n "$(inferd_pid)" ]; then
    echo "inferd already running (pid $(inferd_pid), health=$(health $INFERD_PORT))"
    return 0
  fi
  [ -x "$ROCO_BIN-inferd" ] || { echo "ERROR: $ROCO_BIN-inferd not built — run: cargo build --release -p roco-inferd"; exit 1; }
  echo "starting inferd on :$INFERD_PORT ..."
  nohup "$ROCO_BIN-inferd" --port "$INFERD_PORT" > "$INFERD_LOG" 2>&1 &
  echo "  pid $! — log: $INFERD_LOG"
  echo "  model load takes 30s-5min on first run; use 'roco-stack.sh wait' to block until ready"
}

start_gateway() {
  if [ -n "$(gateway_pid)" ]; then
    echo "gateway already running (pid $(gateway_pid), health=$(health $GATEWAY_PORT))"
    return 0
  fi
  [ -x "$ROCO_BIN" ] || { echo "ERROR: $ROCO_BIN not built — run: cargo build --release -p roco-cli"; exit 1; }
  echo "starting gateway on :$GATEWAY_PORT ..."
  nohup "$ROCO_BIN" gateway --detach --port="$GATEWAY_PORT" --_child-gateway > "$GATEWAY_LOG" 2>&1 &
  echo "  pid $! — log: $GATEWAY_LOG"
}

case "${1:-up}" in
  up)
    start_inferd
    start_gateway
    ;;
  restart)
    pkill -f "roco-inferd --port" 2>/dev/null
    pkill -f "roco gateway --detach" 2>/dev/null
    sleep 2
    start_inferd
    start_gateway
    ;;
  wait)
    # Phase 1: confirm the PROCESS is alive (nothing to poll if it never started)
    for i in $(seq 1 30); do
      if [ -n "$(inferd_pid)" ]; then break; fi
      if [ $i -eq 30 ]; then echo "ERROR: inferd process never started — tail $INFERD_LOG"; exit 1; fi
      sleep 2
    done
    echo "inferd process alive (pid $(inferd_pid)) — waiting for model load / HTTP health..."
    # Phase 2: bounded HTTP poll
    for i in $(seq 1 60); do
      h=$(health $INFERD_PORT)
      echo "[$(date +%H:%M:%S)] inferd :$INFERD_PORT -> $h"
      [ "$h" = ok ] && { echo "inferd ready."; exit 0; }
      sleep 10
    done
    echo "ERROR: inferd not healthy within 10m — tail $INFERD_LOG"
    exit 1
    ;;
  status)
    echo "inferd  :$INFERD_PORT -> $(health $INFERD_PORT)   (pid $(inferd_pid || echo none))"
    echo "gateway :$GATEWAY_PORT -> $(health $GATEWAY_PORT)   (pid $(gateway_pid || echo none))"
    ;;
  logs)
    echo "═══ inferd ($INFERD_LOG) ═══"; tail -15 "$INFERD_LOG"
    echo; echo "═══ gateway ($GATEWAY_LOG) ═══"; tail -15 "$GATEWAY_LOG"
    ;;
  down)
    pkill -f "roco-inferd --port" 2>/dev/null
    pkill -f "roco gateway --detach" 2>/dev/null
    echo "stopped inferd + gateway"
    ;;
  *)
    echo "usage: $0 {up|restart|status|wait|logs|down}"; exit 2;;
esac
