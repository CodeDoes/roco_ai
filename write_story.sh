#!/usr/bin/env bash
# write_story.sh — Write a story with RoCo AI's resumable CLI interface
#
# Demonstrates: interactive prompt, session persistence, resume
#
# Usage:
#   ./write_story.sh          → Write a story (interactive)
#   ./write_story.sh --resume  → Resume last session
#
# This uses the inference API directly. The backend (roco-inferd + gateway)
# must be running. See dev.sh or:
#   RWKV_MODEL=models/rwkv7-...st roco-inferd &
#   roco gateway --detach

set -euo pipefail

ROCO_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$ROCO_DIR"

SESSION_DIR=".roco/sessions"
mkdir -p "$SESSION_DIR"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
DIM='\033[2m'
RESET='\033[0m'

info()  { echo -e "${BLUE}ℹ${RESET} $1"; }
ok()    { echo -e "${GREEN}✓${RESET} $1"; }
header(){ echo -e "\n${CYAN}══ $1 ══${RESET}"; }
ai()    { echo -e "${DIM}${1}${RESET}"; }

API="http://127.0.0.1:8000/v1/completions"

# ── Ensure backend is running ───────────────────────────────────────────
if ! curl -sf http://127.0.0.1:8000/health >/dev/null 2>&1; then
    echo "Starting daemons..."
    RWKV_MODEL="models/rwkv7-g1h-2.9b-20260710-ctx10240-f16.st" \
        nohup ~/.cache/roco_target/release/roco-inferd > /tmp/roco-inferd.log 2>&1 &
    echo "Waiting for inference daemon (30s)..."
    sleep 30
    ~/.cache/roco_target/release/roco gateway --detach 2>/dev/null || true
    sleep 2
fi

# ── Check health ────────────────────────────────────────────────────────
if ! curl -sf http://127.0.0.1:8000/health >/dev/null 2>&1; then
    echo "❌ Backend not available. Start it with:"
    echo "   RWKV_MODEL=models/...st ./dev.sh"
    exit 1
fi
ok "Backend online"

# ── Call the inference API ──────────────────────────────────────────────
call_api() {
    local prompt="$1"
    local system="$2"
    local max_tokens="${3:-300}"
    local temp="${4:-0.8}"

    curl -s -m 120 -X POST "$API" \
        -H "Content-Type: application/json" \
        -d "{
            \"prompt\": $(printf '%s' "$prompt" | jq -Rs .),
            \"system\": $(printf '%s' "$system" | jq -Rs .),
            \"max_tokens\": $max_tokens,
            \"temperature\": $temp
        }" | jq -r '.choices[0].text'
}

# ── Get or create session ───────────────────────────────────────────────
SESSION_ID="story_$(date +%Y%m%d_%H%M%S)"
SESSION_FILE="$SESSION_DIR/${SESSION_ID}.json"

if [[ "${1:-}" == "--resume" ]]; then
    # Find the most recent session
    LAST=$(ls -t "$SESSION_DIR"/*.json 2>/dev/null | head -1)
    if [[ -z "$LAST" ]]; then
        echo "No sessions to resume."
        exit 1
    fi
    SESSION_FILE="$LAST"
    SESSION_ID=$(basename "$LAST" .json)
    info "Resuming session: $SESSION_ID"
else
    info "New session: $SESSION_ID"
fi

# ── Load existing messages if resuming ──────────────────────────────────
declare -a MESSAGES=()
if [[ -f "$SESSION_FILE" ]]; then
    while IFS= read -r line; do
        MESSAGES+=("$line")
    done < <(jq -c '.messages[]' "$SESSION_FILE" 2>/dev/null || echo "")
fi

# ── Save session function ───────────────────────────────────────────────
save_session() {
    local msgs_json="["
    local first=true
    for msg in "${MESSAGES[@]}"; do
        if $first; then
            first=false
        else
            msgs_json+=","
        fi
        msgs_json+="$msg"
    done
    msgs_json+="]"

    cat > "$SESSION_FILE" <<EOF
{
  "id": "$SESSION_ID",
  "messages": $msgs_json,
  "pacing": "auto-accept",
  "created_at": "$(date -Iseconds)",
  "updated_at": "$(date -Iseconds)"
}
EOF
    ok "Session saved: $(basename "$SESSION_FILE")"
}

# ── Build system prompt ─────────────────────────────────────────────────
SYSTEM="You are a creative writing assistant. Respond with vivid, engaging prose. Keep responses focused and atmospheric."

# ── Write the story ─────────────────────────────────────────────────────
header "Writing a Story: The Lighthouse Keeper's Prophecy"

echo ""
info "Theme: A lighthouse keeper on a remote island discovers a message"
info "       in a bottle that predicts the future."
echo ""

# ── Chapter 1: The Discovery ────────────────────────────────────────────
echo -e "\n${CYAN}── Chapter 1: The Discovery ──${RESET}"

PROMPT="Write the opening scene: Elias, a lighthouse keeper on the remote Isle of Ember, discovers a bottle washed ashore during a storm. Inside is a message that predicts a shipwreck — before it happens. Atmospheric, vivid prose. Around 250 words."

RESPONSE=$(call_api "$PROMPT" "$SYSTEM" 400 0.85)

# Save the exchange
MESSAGES+=("{\"role\":\"user\",\"content\":$(printf '%s' "$PROMPT" | jq -Rs .),\"timestamp\":\"$(date -Iseconds)\"}")
MESSAGES+=("{\"role\":\"assistant\",\"content\":$(printf '%s' "$RESPONSE" | jq -Rs .),\"timestamp\":\"$(date -Iseconds)\"}")

ai "$RESPONSE"
echo ""
save_session

# ── Chapter 2: The Second Message ────────────────────────────────────────
echo -e "\n${CYAN}── Chapter 2: The Second Message ──${RESET}"

PROMPT="Continue the story. Elias finds a second bottle with a longer message that describes the lighthouse itself in exact detail — things nobody on the mainland could know. Continue the atmospheric tone. Around 250 words."

RESPONSE=$(call_api "$PROMPT" "$SYSTEM" 400 0.85)

MESSAGES+=("{\"role\":\"user\",\"content\":$(printf '%s' "$PROMPT" | jq -Rs .),\"timestamp\":\"$(date -Iseconds)\"}")
MESSAGES+=("{\"role\":\"assistant\",\"content\":$(printf '%s' "$RESPONSE" | jq -Rs .),\"timestamp\":\"$(date -Iseconds)\"}")

ai "$RESPONSE"
echo ""
save_session

# ── Chapter 3: The Revelation ────────────────────────────────────────────
echo -e "\n${CYAN}── Chapter 3: The Revelation ──${RESET}"

PROMPT="Write the final chapter. Elias realizes the messages are from his future self, sent back through time to warn him of an approaching catastrophe that only the lighthouse can prevent. End with him climbing the lighthouse stairs, knowing exactly what he must do. Around 300 words."

RESPONSE=$(call_api "$PROMPT" "$SYSTEM" 500 0.9)

MESSAGES+=("{\"role\":\"user\",\"content\":$(printf '%s' "$PROMPT" | jq -Rs .),\"timestamp\":\"$(date -Iseconds)\"}")
MESSAGES+=("{\"role\":\"assistant\",\"content\":$(printf '%s' "$RESPONSE" | jq -Rs .),\"timestamp\":\"$(date -Iseconds)\"}")

ai "$RESPONSE"
echo ""
save_session

# ── Save final story to a markdown file ──────────────────────────────────
header "Publishing Story"
STORY_FILE=".roco/stories/${SESSION_ID}.md"
mkdir -p ".roco/stories"

{
    echo "# The Lighthouse Keeper's Prophecy"
    echo ""
    echo "*Generated with RoCo AI — Session: $SESSION_ID*"
    echo ""
    
    for msg in "${MESSAGES[@]}"; do
        role=$(echo "$msg" | jq -r '.role')
        content=$(echo "$msg" | jq -r '.content')
        if [[ "$role" == "assistant" ]]; then
            echo ""
            echo "$content"
            echo ""
            echo "---"
            echo ""
        fi
    done
} > "$STORY_FILE"

ok "Story published: $STORY_FILE"
echo ""
info "Session saved — resume anytime with:"
info "  roco interact --resume $SESSION_ID"
echo ""
info "Or continue with the CLI:"
echo "  roco \"Continue the story of Elias the lighthouse keeper\""
echo ""
