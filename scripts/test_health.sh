#!/usr/bin/env bash
# Test inferd health and backend info
# Usage: ./scripts/test_health.sh
set -euo pipefail

PORT="${ROCO_INFERD_PORT:-18080}"

echo "=== Inferd health check (port ${PORT}) ==="
RESP=$(curl -s "http://127.0.0.1:${PORT}/health" 2>/dev/null || echo '{"status":"unreachable"}')
echo "$RESP" | python3 -m json.tool 2>/dev/null || echo "$RESP"

echo ""
echo "=== Active jobs ==="
curl -s "http://127.0.0.1:${PORT}/jobs" 2>/dev/null | python3 -m json.tool 2>/dev/null || echo "unreachable"
