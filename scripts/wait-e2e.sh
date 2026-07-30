#!/usr/bin/env bash
# Wait for a roco story pipeline to finish.
# Polls both the tmux pane and the agent journal every 10 seconds.
# Exits when the pipeline completes or fails.
set -euo pipefail

SESSION="${1:-roco-e2e}"
MAX_POLLS="${2:-120}"  # 120 * 10s = 20 min max

echo "Waiting for pipeline in tmux session '$SESSION'..."
echo "Polling every 10s (max ${MAX_POLLS}x = $((MAX_POLLS * 10 / 60)) min)..."
echo ""

for i in $(seq 1 "$MAX_POLLS"); do
    sleep 10

    # Check if tmux session still exists
    if ! tmux has-session -t "$SESSION" 2>/dev/null; then
        echo "❌ tmux session '$SESSION' is gone."
        exit 1
    fi

    # Capture the last 5 lines of the pane
    OUTPUT=$(tmux capture-pane -t "$SESSION:0.0" -p -S -5 2>/dev/null || true)

    # Check for completion markers
    if echo "$OUTPUT" | grep -q "✅ Done!"; then
        echo "✅ Pipeline completed successfully!"
        echo ""
        tmux capture-pane -t "$SESSION:0.0" -p -S -20
        exit 0
    fi

    # Check for failure markers
    if echo "$OUTPUT" | grep -q "❌ Story pipeline failed"; then
        echo "❌ Pipeline FAILED!"
        echo ""
        tmux capture-pane -t "$SESSION:0.0" -p -S -40
        exit 1
    fi

    # Print progress (last non-empty line)
    PROGRESS=$(journalctl 2>/dev/null || echo "")
    if [ -f .roco/agent-journal.md ]; then
        LAST=$(tail -3 .roco/agent-journal.md 2>/dev/null | grep -v "^$" | tail -1 || echo "")
        if [ -n "$LAST" ]; then
            echo "[$i] $LAST"
        else
            echo "[$i] $(echo "$OUTPUT" | grep -E "✓|✍️|📝|📚|📋|📦" | tail -1 || echo "running...")"
        fi
    else
        echo "[$i] $(echo "$OUTPUT" | grep -E "✓|✍️|📝|📚|📋|📦" | tail -1 || echo "running...")"
    fi
done

echo "⏰ Timed out after $((MAX_POLLS * 10 / 60)) min."
echo ""
tmux capture-pane -t "$SESSION:0.0" -p -S -30
exit 1
