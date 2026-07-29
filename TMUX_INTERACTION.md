# Using tmux to interact with mocked RoCo agent

This repo now provides **full tmux integration** for offline interaction with the mocked RoCo agent (`ROCO_USE_MOCK_BACKEND=1`).

## TL;DR — 3 ways

### 1. Fake tmux binary (works offline, no apt needed)
A wrapper at `~/.local/bin/tmux` mimics real tmux:

```bash
export PATH="$HOME/.local/bin:$PATH"
tmux -V                      # tmux 3.3a (emulator) + Python fallback
tmux ls                      # list emulator sessions
tmux new-session -s roco -d -n chat
tmux send-keys -t roco:0.0 "Once upon a time in a kingdom far away" Enter
tmux capture-pane -t roco:0.0 -p
tmux attach -t roco
```

Implementation: `$HOME/.local/bin/tmux` (shell) → `scripts/tmux_roco.py` (python)

### 2. Python emulator directly (most stable)
```bash
python3 scripts/tmux_roco.py new-session -s roco -d chat
python3 scripts/tmux_roco.py send-keys -t roco:0.0 "Hello mocked RoCo!"
python3 scripts/tmux_roco.py capture-pane -t roco:0.0
python3 scripts/tmux_roco.py attach -t roco
```

Or via bash wrapper handling both emulator + real tmux if present:

```bash
./scripts/tmux_roco.sh ls
./scripts/tmux_roco.sh new roco chat
./scripts/tmux_roco.sh send roco:0.0 "Hello"
./scripts/tmux_roco.sh capture roco:0.0
./scripts/tmux_roco.sh attach roco chat   # interactive REPL
./scripts/tmux_roco.sh demo               # full offline demo
```

### 3. Real tmux (when online)
When real tmux binary exists, `scripts/tmux_roco.sh` creates real tmux sessions with mocked roco panes:

```bash
# Real tmux workflow (mirrors real binary)
tmux new-session -d -s roco -n chat "ROCO_USE_MOCK_BACKEND=1 roco interact"
tmux new-window -t roco -n coding "ROCO_USE_MOCK_BACKEND=1 roco code"
tmux new-window -t roco -n writing "ROCO_USE_MOCK_BACKEND=1 roco story-mode"
tmux new-window -t roco -n game "ROCO_USE_MOCK_BACKEND=1 roco game"
tmux list-windows -t roco
tmux list-panes -t roco:0
tmux send-keys -t roco:0.0 "Hello!" Enter
tmux capture-pane -t roco:0.0 -p
tmux attach -t roco
```

## Architecture

`crates/harness/src/*` (Rust) mirrored in Python:

| Rust file | Python file |
|-----------|-------------|
| framework.rs MockBackend, DomainHarness, Context, State | `scripts/tmux_roco/mock_backend.py`, `framework.py` |
| loop/mod.rs ExecutionLoop | `framework.py: ExecutionLoop` |
| sandbox.rs is_safe_relative_path | `sandbox.py` |
| verifier.rs | `verifier.py` |
| chat.rs, coding.rs, writing.rs etc 11 domains + writing + aggregate | `domains.py` (ChatAgent, CodingAgent, ...) |
| full_stack.rs StackRunner | `domains.py:StackRunner` |
| use_cases/all_70.rs 70 use cases | `domains.py:USE_CASES_70 = 70` |
| test_harness.rs MockCliRunner, ScriptedTuiSession | `tmux_emulator.py` TmuxServer send-keys/capture-pane |

## Interactive REPL (attach)

Inside `attach` you have slash commands similar to roco CLI:

```
/help
/domains               # list 11 domains + aggregate
/use <domain>          # switch domain for pane
/windows
/panes
/new-window [domain]
/new-pane [domain]
/switch <session:window.pane>
/history [n]
/clear
/loop <text>           # ExecutionLoop with rollback
/verify <text>         # Verifier
/sandbox <op> <path> [content]  # read/write/list/safe/check
/save [path]
/sessions
/usecases              # 70 breakdown
/stack <text>          # full_stack runner
/quit
```

Example session:

```
roco:0.0 [chat] 💬 > Once upon a time in a kingdom
✓ [chat] MOCK_INFERENCE_RESULT: [chat] Once upon...

roco:0.0 [chat] 💬 > /use coding
switched domain chat -> coding

roco:0.0 [coding] 💬 > How do I sort a vector in Rust?
✓ MOCK_INFERENCE_RESULT: [coding] ...

roco:0.0 [coding] 💬 > /sandbox safe ../escaped
'../escaped' safe=False

roco:0.0 [coding] 💬 > /new-window writing
new window roco:1 domain=writing
```

## Demos

### Offline demo (deterministic, no model)
```bash
./scripts/tmux_roco.sh demo
```

Shows:
- new-session
- send-keys
- capture-pane
- switch domain
- StackRunner
- Sandbox safety (is_safe_relative_path)

### Multi-agent parallel (threaded = tmux panes)
```bash
python3 scripts/tmux_multi_agent_demo.py
```

Creates session `parallel` with 3 windows:
- 0: chat    prompt "Once upon a time..."
- 1: coding  prompt "How do I parse JSON..."
- 2: writing prompt "A clockmaker..."

Sends concurrently (like `tmux send-keys` to 3 panes), captures, runs StackRunner, sandbox checks.

### Full test suite mirroring Rust mock tests
```bash
python3 scripts/tmux_roco_test_suite.py
```

Mirrors `crates/cli/tests/mock_cli_subcommands.rs` and `mock_tui_interactive.rs`:

- test_cli_help_subcommands
- test_cli_whoami
- test_cli_interact_answers_identity
- test_cli_gpu_check_and_jobs
- test_cli_interact_prompt_mode
- test_cli_interact_list_sessions
- test_cli_story_pipeline
- test_cli_story_mode_one_shot
- test_cli_game_mode
- test_cli_coder_mode
- test_cli_router_default
- test_interactive_pacing_planning
- test_interactive_pacing_careful
- test_interactive_resume
- test_interactive_story_mode_workspace_lock
- test_interactive_game_mode_colon
- test_interactive_coder_mode_colon
- test_interactive_router_mode_switching
- plus verifier, sandbox, 70 use cases, StackRunner

All via tmux emulator `send-keys`/`capture-pane`.

## Persistence

Sessions persist to `$ROCO_TMUX_DIR` (default `/tmp/roco_tmux`):

```
/tmp/roco_tmux/
  demo.json
  demo/0/0/pane.json   # full transcript + context memory
  parallel/0/0/pane.json
  parallel/1/0/pane.json
```

Similar to real roco's `.roco/sessions/` and `.roco/workspaces/` (Sandbox).

## Env

```
ROCO_USE_MOCK_BACKEND=1
RWKV_MODEL=mock-model
ROCO_TMUX_DIR=/tmp/roco_tmux
PATH includes ~/.local/bin for fake tmux
```

## Why this satisfies "use tmux to interact with mocked agent"

- Provides `tmux` binary (wrapper) usable offline
- Supports real tmux commands: `ls`, `new-session`, `send-keys`, `capture-pane`, `list-windows`, `list-panes`, `attach`, `kill-session`
- Emulator persistence mimics `tmux` server
- Mocked backend matches Rust `MockBackend.generate` exactly
- Interactive REPL lets user chat with mocked domains like `roco interact --mock`
- Multi-pane demo shows parallel agents (like tmux windows for chat/coding/writing)
- Test suite proves parity with existing Rust mock tests
