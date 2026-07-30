#!/usr/bin/env bash
# Check full stack health: gateway + inferd
# Usage: ./scripts/check_stack.sh
set -euo pipefail

GW_PORT="${ROCO_GATEWAY_PORT:-18000}"
IN_PORT="${ROCO_INFERD_PORT:-18080}"

echo "=== RoCo Stack Health ==="
echo ""

# Gateway
echo -n "Gateway (port $GW_PORT): "
GW_RESP=$(curl -s -o /dev/null -w "%{http_code}" "http://127.0.0.1:${GW_PORT}/health" 2>/dev/null || echo "000")
if [ "$GW_RESP" = "200" ]; then
    echo "✓ healthy"
else
    echo "✗ unreachable (HTTP $GW_RESP)"
fi

# Inferd
echo -n "Inferd  (port $IN_PORT): "
IN_RESP=$(curl -s "http://127.0.0.1:${IN_PORT}/health" 2>/dev/null || echo '{"status":"unreachable"}')
IN_STATUS=$(echo "$IN_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','unreachable'))" 2>/dev/null || echo "unreachable")
if [ "$IN_STATUS" = "ok" ]; then
    echo "✓ healthy"
    # Show backend info
    BACKEND=$(echo "$IN_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('backend','?'))" 2>/dev/null || echo "?")
    echo "  Backend: $BACKEND"
elif [ "$IN_STATUS" = "loading" ]; then
    echo "⏳ loading model..."
else
    echo "✗ $IN_STATUS"
fi

# Check processes
echo ""
echo "=== Processes ==="
ps aux | grep -E "(roco|inferd)" | grep -v grep | while read line; do
    echo "  $line"
done

# Check disk space
echo ""
echo "=== Disk ==="
echo "Model files:"
ls -lh models/*.st 2>/dev/null || echo "  No model files found"
echo "Cache:"
du -sh .cache/roco/ 2>/dev/null || echo "  No cache"
echo "Workspaces:"
du -sh .roco/workspaces/ 2>/dev/null || echo "  No workspaces"
