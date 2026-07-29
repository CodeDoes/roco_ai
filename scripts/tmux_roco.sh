#!/usr/bin/env bash
# tmux_roco.sh — use tmux to interact with mocked roco agent
# If real tmux exists, spins up a real tmux session with multiple panes
# running mocked roco agents via Python emulator (or cargo binary if present).
# If tmux missing, falls back to Python emulator interactive REPL.
#
# Usage:
#   ./scripts/tmux_roco.sh                  # interactive attach to roco session
#   ./scripts/tmux_roco.sh ls               # list sessions
#   ./scripts/tmux_roco.sh new roco chat    # new session
#   ./scripts/tmux_roco.sh send roco:0.0 "hello"
#   ./scripts/tmux_roco.sh capture roco
#   ./scripts/tmux_roco.sh kill roco
#   ./scripts/tmux_roco.sh attach roco
#   ./scripts/tmux_roco.sh real-attach      # force real tmux attach
#
# Env:
#   ROCO_USE_MOCK_BACKEND=1 (always forced here)
#   RWKV_MODEL=mock-model
#   ROCO_TMUX_DIR=/tmp/roco_tmux

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
export ROCO_USE_MOCK_BACKEND=1
export RWKV_MODEL=mock-model
export ROCO_TMUX_DIR="${ROCO_TMUX_DIR:-/tmp/roco_tmux}"
mkdir -p "$ROCO_TMUX_DIR"

PYTHON="${PYTHON:-python3}"
EMULATOR="$SCRIPT_DIR/tmux_roco.py"
export PYTHONPATH="$SCRIPT_DIR:${PYTHONPATH:-}"

# Check if cargo binary exists for real roco
ROCO_BIN=""
if [[ -x "$ROOT_DIR/target/debug/roco" ]]; then
    ROCO_BIN="$ROOT_DIR/target/debug/roco"
elif command -v roco >/dev/null 2>&1; then
    ROCO_BIN="roco"
fi

has_tmux=false
if command -v tmux >/dev/null 2>&1; then
    # Differentiate real vs fake emulator wrapper: fake reports "emulator" in -V
    if tmux -V 2>&1 | grep -qi emulator; then
        has_tmux=false
        FAKE_TMUX=1
    else
        has_tmux=true
    fi
fi
export FAKE_TMUX=${FAKE_TMUX:-0}

cmd="${1:-attach}"
case "$cmd" in
    ls|list|list-sessions|lss)
        echo "=== Emulator sessions ==="
        $PYTHON "$EMULATOR" list-sessions
        if $has_tmux; then
            echo ""
            echo "=== Real tmux sessions ==="
            tmux ls 2>&1 || echo "no real tmux sessions"
        fi
        ;;

    new|new-session|new-sess)
        SESS="${2:-roco}"
        DOMAIN="${3:-chat}"
        echo "Creating new emulator session $SESS domain=$DOMAIN"
        $PYTHON "$EMULATOR" new-session -s "$SESS" -d "$DOMAIN"
        if $has_tmux; then
            echo ""
            echo "Creating real tmux session $SESS with panes for each domain"
            # Kill if exists (for idempotency)
            tmux kill-session -t "$SESS" 2>/dev/null || true
            # First window: chat
            if [[ -n "$ROCO_BIN" ]]; then
                tmux new-session -d -s "$SESS" -n chat "ROCO_USE_MOCK_BACKEND=1 $ROCO_BIN interact --pace careful; bash"
            else
                tmux new-session -d -s "$SESS" -n chat "$PYTHON $EMULATOR attach -t $SESS"
            fi
            # Additional windows
            for d in coding writing browser email research html pet debug; do
                if [[ -n "$ROCO_BIN" ]]; then
                    tmux new-window -t "$SESS" -n "$d" "ROCO_USE_MOCK_BACKEND=1 $ROCO_BIN $d; bash" 2>/dev/null || true
                else
                    tmux new-window -t "$SESS" -n "$d" "$PYTHON $EMULATOR attach -t ${SESS}_${d} --domain $d; bash" 2>/dev/null || true
                fi
            done
            echo "Real tmux session $SESS created with windows:"
            tmux list-windows -t "$SESS"
        fi
        ;;

    send|send-keys|sk)
        TARGET="${2:-roco:0.0}"
        shift 2 || true
        MSG="$*"
        if [[ -z "$MSG" ]]; then
            echo "usage: $0 send <target> <message>"
            exit 1
        fi
        echo "Emulator send-keys $TARGET: $MSG"
        $PYTHON "$EMULATOR" send-keys -t "$TARGET" "$MSG"
        if $has_tmux; then
            echo ""
            echo "Real tmux send-keys $TARGET: $MSG"
            tmux send-keys -t "$TARGET" "$MSG" Enter 2>&1 || echo "real tmux send failed (pane may not exist)"
        fi
        ;;

    capture|capture-pane|cap|cp)
        TARGET="${2:-roco}"
        echo "=== Emulator capture-pane $TARGET ==="
        $PYTHON "$EMULATOR" capture-pane -t "$TARGET"
        if $has_tmux; then
            echo ""
            echo "=== Real tmux capture-pane $TARGET ==="
            tmux capture-pane -t "$TARGET" -p 2>&1 || echo "no real pane"
        fi
        ;;

    kill|kill-session|ks)
        TARGET="${2:-roco}"
        echo "Killing emulator session $TARGET"
        $PYTHON "$EMULATOR" kill-session -t "$TARGET" || true
        if $has_tmux; then
            echo "Killing real tmux session $TARGET"
            tmux kill-session -t "$TARGET" 2>&1 || echo "no real session"
        fi
        ;;

    list-windows|lsw)
        TARGET="${2:-roco}"
        echo "=== Emulator list-windows $TARGET ==="
        $PYTHON "$EMULATOR" list-windows -t "$TARGET"
        if $has_tmux; then
            echo ""
            echo "=== Real tmux list-windows ==="
            tmux list-windows -t "$TARGET" 2>&1 || echo "no real session"
        fi
        ;;

    list-panes|lsp)
        TARGET="${2:-roco:0}"
        echo "=== Emulator list-panes $TARGET ==="
        $PYTHON "$EMULATOR" list-panes -t "$TARGET"
        if $has_tmux; then
            echo ""
            echo "=== Real tmux list-panes ==="
            tmux list-panes -t "$TARGET" 2>&1 || echo "no real window"
        fi
        ;;

    attach|att|a|interactive|repl)
        TARGET="${2:-roco}"
        DOMAIN="${3:-chat}"
        echo "Attaching via emulator to $TARGET domain=$DOMAIN"
        echo "For real tmux attach, run: $0 real-attach $TARGET"
        echo ""
        # Check if we want to highlight real tmux usage
        if $has_tmux; then
            echo "Real tmux available — you can also run:"
            echo "  tmux attach -t $TARGET"
            echo "  tmux new-session -s $TARGET"
            echo "  tmux send-keys -t $TARGET:0.0 \"hello\" Enter"
            echo "  tmux capture-pane -t $TARGET -p"
            echo ""
        fi
        exec $PYTHON "$EMULATOR" attach -t "$TARGET" --domain "$DOMAIN"
        ;;

    real-attach|rat)
        TARGET="${2:-roco}"
        if ! $has_tmux; then
            echo "Real tmux binary not found — falling back to emulator"
            exec $PYTHON "$EMULATOR" attach -t "$TARGET"
        fi
        exec tmux attach -t "$TARGET"
        ;;

    domains)
        $PYTHON "$EMULATOR" domains
        ;;

    usecases)
        $PYTHON "$EMULATOR" usecases
        ;;

    demo)
        echo "=== RoCo Mock Agent tmux Demo (offline) ==="
        echo ""
        echo "Step 1: create emulator session 'demo' with chat domain"
        $PYTHON "$EMULATOR" new-session -s demo -d chat
        echo ""
        echo "Step 2: list sessions"
        $PYTHON "$EMULATOR" list-sessions
        echo ""
        echo "Step 3: send keys to demo:0.0 — 'Once upon a time in a kingdom far away'"
        $PYTHON "$EMULATOR" send-keys -t demo:0.0 "Once upon a time in a kingdom far away"
        echo ""
        echo "Step 4: capture pane"
        $PYTHON "$EMULATOR" capture-pane -t demo:0.0
        echo ""
        echo "Step 5: switch domain to coding and ask question"
        $PYTHON "$EMULATOR" send-keys -t demo:0.0 "/use coding" || true
        # Direct python call for demo since slash commands only inside attach
        PYTHONPATH="$SCRIPT_DIR:$PYTHONPATH" $PYTHON -c "
from tmux_roco.tmux_emulator import TmuxServer
s=TmuxServer()
sess=s.get_session('demo')
if not sess:
    sess=s.new_session('demo', domain='coding')
pane=sess.get_window('0').get_pane('0')
pane.switch_domain('coding')
out,ok=pane.send('How do I sort a vector in Rust?')
print(out)
"
        echo ""
        echo "Step 6: run full stack runner"
        PYTHONPATH="$SCRIPT_DIR:$PYTHONPATH" $PYTHON -c "
from tmux_roco.domains import StackRunner
res=StackRunner.run_all('A clockmaker builds a device that freezes time')
print(f'success={res.success} attempts={res.attempts} rollback={res.rollback_count}')
print(res.output)
"
        echo ""
        echo "Step 7: sandbox safety checks"
        PYTHONPATH="$SCRIPT_DIR:$PYTHONPATH" $PYTHON -c "
from tmux_roco.sandbox import is_safe_relative_path, Sandbox
from pathlib import Path
import tempfile, os
print('is_safe_relative_path tests:')
tests=['foo/bar.txt','code.rs','../escaped.txt','./local.txt','/etc/passwd']
for t in tests:
    print(f'  {t!r}: {is_safe_relative_path(t)}')
tmp=tempfile.mkdtemp()
sb=Sandbox(tmp)
sb.write('test.txt','hello')
print(f'write/read test.txt: {sb.read(\"test.txt\")}')
print(f'list: {sb.list_files()}')
"
        echo ""
        echo "Demo complete — session 'demo' persists in $ROCO_TMUX_DIR"
        ;;

    help|--help|-h)
        echo "tmux_roco.sh — tmux + mocked RoCo agent"
        echo ""
        echo "Usage: $0 <command> [args]"
        echo "Commands:"
        echo "  ls | list-sessions                     List emulator + real tmux sessions"
        echo "  new [session] [domain]                 Create new session (emulator + real tmux if available)"
        echo "  send <target> <message>                Send keys to pane"
        echo "  capture <target>                       Capture pane scrollback"
        echo "  kill <session>                         Kill session"
        echo "  list-windows <session>                 List windows"
        echo "  list-panes <session:window>            List panes"
        echo "  attach [session] [domain]              Attach interactive REPL (emulator)"
        echo "  real-attach [session]                  Attach real tmux"
        echo "  domains                                List agent domains"
        echo "  usecases                               List 70 use cases"
        echo "  demo                                   Run offline demo sequence"
        echo ""
        echo "Targets format (like tmux):"
        echo "  session:window.pane   e.g. roco:0.0"
        echo "  session:window        e.g. roco:1"
        echo "  :window.pane          current session"
        echo ""
        echo "Interactive REPL slash commands:"
        echo "  /use <domain>, /switch <tgt>, /history, /loop <text>, etc."
        echo "  Type $PYTHON $EMULATOR attach --help for more"
        echo ""
        if $has_tmux; then
            echo "Real tmux detected: $(tmux -V)"
        else
            echo "Real tmux NOT detected (offline sandbox) — using pure Python emulator"
            echo "To install tmux when online: sudo apt-get install tmux"
        fi
        ;;

    *)
        echo "Unknown command $cmd — running Python emulator with args: $cmd $*"
        exec $PYTHON "$EMULATOR" "$@"
        ;;
esac
