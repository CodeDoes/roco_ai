#!/usr/bin/env bash
# Full story generation test with timeout and progress
# Usage: ./scripts/test_story.sh [premise] [timeout_secs]
set -euo pipefail

PREMISE="${1:-A cat finds a door}"
TIMEOUT="${2:-600}"
WORKSPACE_DIR=".roco/workspaces"

echo "=== Story generation test ==="
echo "Premise: $PREMISE"
echo "Timeout: ${TIMEOUT}s"
echo ""

# Clean old story processes
pkill -f "roco story" 2>/dev/null || true
sleep 1

# Find the workspace we'll create by watching the directory
BEFORE=$(ls "$WORKSPACE_DIR" 2>/dev/null | sort -r | head -1)

START=$(date +%s)

# Run story with timeout, capturing output
timeout "$TIMEOUT" cargo run --bin roco -- story "$PREMISE" 2>&1 &
STORY_PID=$!

# Monitor progress
LAST_LINE=""
while kill -0 "$STORY_PID" 2>/dev/null; do
    ELAPSED=$(( $(date +%s) - START ))
    
    # Check latest journal entry
    NEW_LINE=$(tail -1 .roco/agent-journal.md 2>/dev/null || echo "")
    if [ "$NEW_LINE" != "$LAST_LINE" ] && [ -n "$NEW_LINE" ]; then
        echo "[${ELAPSED}s] $NEW_LINE"
        LAST_LINE="$NEW_LINE"
    fi
    
    # Check workspace for new files
    AFTER=$(ls "$WORKSPACE_DIR" 2>/dev/null | sort -r | head -1)
    if [ "$AFTER" != "$BEFORE" ] && [ -n "$AFTER" ]; then
        echo "[${ELAPSED}s] Workspace: $WORKSPACE_DIR/$AFTER"
        BEFORE="$AFTER"
        
        # Show files as they appear
        for f in "$WORKSPACE_DIR/$AFTER"/*; do
            if [ -f "$f" ] && [ ! -f "/tmp/seen_$(basename "$f")" ]; then
                echo "[${ELAPSED}s] Created: $(basename "$f")"
                touch "/tmp/seen_$(basename "$f")"
            fi
        done
    fi
    
    sleep 2
done

EXIT_CODE=$?
ELAPSED=$(( $(date +%s) - START ))

# Clean up seen markers
rm -f /tmp/seen_*.md 2>/dev/null

echo ""
echo "=== Result ==="
echo "Exit code: $EXIT_CODE"
echo "Elapsed: ${ELAPSED}s"

# Find the workspace
LATEST=$(ls -t "$WORKSPACE_DIR" 2>/dev/null | head -1)
if [ -n "$LATEST" ]; then
    echo "Workspace: $WORKSPACE_DIR/$LATEST"
    echo "Files:"
    ls -la "$WORKSPACE_DIR/$LATEST/" 2>/dev/null
    
    # Check for chapters
    CHAPTER_COUNT=$(ls "$WORKSPACE_DIR/$LATEST"/0[3-9]*-*.md 2>/dev/null | wc -l)
    echo "Chapters written: $CHAPTER_COUNT"
fi

if [ $EXIT_CODE -eq 124 ]; then
    echo "TIMEOUT after ${TIMEOUT}s"
fi
