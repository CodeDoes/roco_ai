#!/usr/bin/env bash
# Quick smoke test: single completion via HTTP
# Usage: ./scripts/test_complete.sh [prompt]
set -euo pipefail

PROMPT="${1:-Once upon a time}"
PORT="${ROCO_INFERD_PORT:-18080}"

echo "=== Quick completion test (prompt: '$PROMPT') ==="
START=$(date +%s%N)
RESP=$(curl -s -X POST "http://127.0.0.1:${PORT}/v1/completions" \
  -H "Content-Type: application/json" \
  -d "{\"prompt\":\"$PROMPT\",\"max_tokens\":100,\"temperature\":0.5}")
END=$(date +%s%N)
MS=$(( (END - START) / 1000000 ))

echo "Time: ${MS}ms"
echo "$RESP" | python3 -m json.tool 2>/dev/null || echo "$RESP"
