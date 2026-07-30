#!/usr/bin/env bash
# Test structured JSON output (outline, wiki, etc.)
# Tests that the model produces valid JSON without thinking contamination
# Usage: ./scripts/test_structured.sh [timeout_secs]
set -euo pipefail

TIMEOUT="${1:-120}"
PORT="${ROCO_INFERD_PORT:-18080}"

echo "=== Structured output test ==="

# Test outline generation
echo ""
echo "--- Test 1: Outline (JSON) ---"
RESP=$(curl -s -X POST "http://127.0.0.1:${PORT}/v1/completions" \
  -H "Content-Type: application/json" \
  -d '{
    "system": "You are a story planner. Output valid JSON only. No thinking.",
    "prompt": "Create a 2-chapter outline for: A cat finds a door. Output JSON matching: {\"title\": \"...\", \"genre\": \"...\", \"chapters\": [{\"number\": 1, \"title\": \"...\", \"summary\": \"...\"}]}",
    "max_tokens": 400,
    "temperature": 0.6
  }')

TEXT=$(echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['choices'][0]['text'])" 2>/dev/null)
echo "Raw output (first 200 chars): ${TEXT:0:200}"
echo ""

# Try to parse JSON
CLEAN=$(echo "$TEXT" | sed 's/<think>.*<\/think>//g' | sed '/^```/d' | sed 's/```json//g' | sed 's/```//g' | sed '/^$/d')
PARSED=$(echo "$CLEAN" | python3 -m json.tool 2>&1)
if [ $? -eq 0 ]; then
    echo "✓ Valid JSON!"
    echo "$PARSED" | head -10
else
    echo "✗ Invalid JSON:"
    echo "$PARSED"
    echo ""
    echo "Cleaned output:"
    echo "$CLEAN"
fi

# Test world bible
echo ""
echo "--- Test 2: World bible (JSON) ---"
RESP=$(curl -s -X POST "http://127.0.0.1:${PORT}/v1/completions" \
  -H "Content-Type: application/json" \
  -d '{
    "system": "You are a worldbuilding assistant. Output valid JSON only. No thinking.",
    "prompt": "Create characters and setting for: A cat finds a door. Output JSON: {\"characters\": [{\"name\": \"...\", \"role\": \"...\", \"description\": \"...\"}], \"settings\": [{\"name\": \"...\", \"description\": \"...\"}]}",
    "max_tokens": 600,
    "temperature": 0.7
  }')

TEXT=$(echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['choices'][0]['text'])" 2>/dev/null)
echo "Raw output (first 200 chars): ${TEXT:0:200}"
echo ""

CLEAN=$(echo "$TEXT" | sed 's/<think>.*<\/think>//g' | sed '/^```/d' | sed 's/```json//g' | sed 's/```//g' | sed '/^$/d')
PARSED=$(echo "$CLEAN" | python3 -m json.tool 2>&1)
if [ $? -eq 0 ]; then
    echo "✓ Valid JSON!"
else
    echo "✗ Invalid JSON:"
    echo "$PARSED"
fi
