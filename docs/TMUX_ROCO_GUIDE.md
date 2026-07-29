# tmux + RoCo Mocked Agent — Guide

This guide shows how to use `tmux` (or the offline Python emulator fallback) to interact with the mocked RoCo agent (`roco` with `ROCO_USE_MOCK_BACKEND=1`).

## Why tmux?

The mocked RoCo harness (`crates/harness` + `roco-cli` `MockBackend`) simulates the RWKV 2.9B model deterministically:

```
MockBackend.generate(prompt) -> "MOCK_INFERENCE_RESULT: {prompt.trim()}"
```

It lets you test:
- DomainHarness lifecycle (`name`, `init`, `run`, `verify`, `rollback`)
- ExecutionLoop retry/rollback detection
- Sandbox path containment (`is_safe_relative_path`)
- Verifier deterministic checks
- Full-stack `StackRunner`

But the real CLI (`roco interact`, `roco code`, `roco game`, etc.) is interactive. `tmux` gives you:
- persistent sessions across disconnects
- multiple panes/windows = multiple domains in parallel
- `send-keys` scripting for automated tests (same pattern as `MockCliRunner` / `ScriptedTuiSession` in `crates/cli/tests/mock_cli_subcommands.rs`)
- `capture-pane` for scrollback/history inspection

## Offline Sandbox Reality

The execution environment for this task has **no `cargo` binary and no `tmux` binary** and no internet to `apt-get install tmux`. The solution is a **pure Python tmux emulator** that mirrors both:

1. The Rust harness (`crates/harness/src/*`) in Python
2. The tmux CLI surface (`new-session`, `send-keys`, `capture-pane`, `attach`)

It lives in `scripts/tmux_roco/`:

```
scripts/tmux_roco/
  __init__.py
  mock_backend.py       # mirrors roco_engine::MockBackend
  framework.py          # DomainHarness, HarnessConfig, Context, State, ExecutionLoop
  sandbox.py            # is_safe_relative_path, Sandbox read/write/allowed
  verifier.py           # Verifier::verify
  domains.py            # 11 domains + writing + aggregate + 70 use-cases + StackRunner
  tmux_emulator.py      # TmuxServer / Session / Window / Pane with persistence
  cli.py                # CLI + interactive REPL
scripts/tmux_roco.py    # entry point
scripts/tmux_roco.sh    # bash wrapper that uses real tmux if available, else emulator
```

## Quick Start (emulator)

```bash
# Make executable
chmod +x scripts/tmux_roco.py scripts/tmux_roco.sh

# Demo sequence — deterministic, no model needed
./scripts/tmux_roco.sh demo

# List sessions (emulator + real tmux if present)
./scripts/tmux_roco.sh ls

# Create new session "roco" with chat domain
./scripts/tmux_roco.sh new roco chat

# Send a message to pane roco:0.0 (session:window.pane like tmux)
./scripts/tmux_roco.sh send roco:0.0 "Once upon a time in a kingdom far away"

# Capture scrollback
./scripts/tmux_roco.sh capture roco:0.0

# Interactive attach — full REPL with slash commands
./scripts/tmux_roco.sh attach roco chat
```

### Inside the REPL (`attach`)

```
roco:0.0 [chat] 💬 > Hello! My name is Ada
✓ [chat] MOCK_INFERENCE_RESULT: [chat] Hello! My name is Ada ctx='roco_0_0_....'

roco:0.0 [chat] 💬 > /domains
Available domains:
  aggregate
  browser
  chat
  coding
  debug
  email
  full_stack
  html
  organization
  pet
  research
  writing

roco:0.0 [chat] 💬 > /use coding
switched domain chat -> coding

roco:0.0 [coding] 💬 > How do I sort a vector in Rust?
✓ [coding] MOCK_INFERENCE_RESULT: [coding] How do I sort a vector...

roco:0.0 [coding] 💬 > /loop A clockmaker builds a device that freezes time
✓ execution loop with retry/rollback

roco:0.0 [coding] 💬 > /sandbox safe ../escaped.txt
'../escaped.txt' safe=False

roco:0.0 [coding] 💬 > /sandbox write test.txt "hello sandbox"
wrote test.txt

roco:0.0 [coding] 💬 > /history 20
[06:33:00] system, user, assistant lines...

roco:0.0 [coding] 💬 > /new-window writing
new window roco:1 domain=writing

roco:0.0 [coding] 💬 > /switch roco:1.0
switched to roco:1.0 domain=writing

roco:0.0 [writing] 💬 > /quit
Goodbye! Session persisted at /tmp/roco_tmux
```

## Real tmux (when tmux binary exists)

If you have real tmux installed (online environment), `scripts/tmux_roco.sh` will **also** create real tmux sessions alongside the emulator:

```bash
# Real tmux flow
tmux new-session -d -s roco -n chat "ROCO_USE_MOCK_BACKEND=1 roco interact"
tmux new-window -t roco -n coding "ROCO_USE_MOCK_BACKEND=1 roco code"
tmux new-window -t roco -n writing "ROCO_USE_MOCK_BACKEND=1 roco story-mode"
tmux list-windows -t roco
tmux list-panes -t roco:0
tmux send-keys -t roco:0.0 "Hello mocked RoCo!" Enter
tmux capture-pane -t roco:0.0 -p
tmux attach -t roco
```

`scripts/tmux_roco.sh` does exactly that logic in `new`, `send`, `capture`, `attach` commands, trying both emulator and real tmux.

### tmux.conf snippet for RoCo

```tmux
# .tmux.conf — fast RoCo workflow
bind-key C-r new-session -s roco -n chat "ROCO_USE_MOCK_BACKEND=1 roco interact"
bind-key C-c new-window -n coding "ROCO_USE_MOCK_BACKEND=1 roco code"
bind-key C-w new-window -n writing "ROCO_USE_MOCK_BACKEND=1 roco story-mode"
```

## Mapping to Rust Harness

| Rust | Python emulator |
|------|-----------------|
| `crates/harness/src/framework.rs` `DomainHarness`, `Context`, `State`, `MockBackend` | `scripts/tmux_roco/framework.py`, `mock_backend.py` |
| `crates/harness/src/loop/mod.rs` `ExecutionLoop` | `framework.py:ExecutionLoop` |
| `crates/harness/src/sandbox.rs` `is_safe_relative_path` | `sandbox.py` |
| `crates/harness/src/verifier.rs` | `verifier.py` |
| `crates/harness/src/chat.rs`, `coding.rs`, `writing.rs`, etc. (11 domains) | `domains.py: ChatAgent`, `CodingAgent`, ... |
| `crates/harness/src/full_stack.rs` `StackRunner` | `domains.py:StackRunner` |
| `crates/harness/src/use_cases/all_70.rs` | `domains.py:USE_CASES_70` |
| `crates/cli/src/test_harness.rs` `MockCliRunner`, `ScriptedTuiSession` | `tmux_emulator.py` `send-keys` / `capture-pane` |
| `crates/cli/tests/mock_cli_subcommands.rs`, `mock_tui_interactive.rs` | `./scripts/tmux_roco.sh demo` |

## Multi-Agent Parallel Demo

```bash
# 3 parallel agents like tmux panes
python3 scripts/tmux_roco.py new-session -s multi -d chat
python3 scripts/tmux_roco.py send-keys -t multi:0.0 "You are a math tutor"
python3 scripts/tmux_multi_agent_demo.py   # spawns chat, coding, writing in threads
```

See `scripts/tmux_multi_agent_demo.py`:

- Creates session `parallel` with 3 windows
- Each pane runs a different domain
- Sends prompts concurrently
- Captures results with rollback counts

## Persistence

All emulator sessions persist to `$ROCO_TMUX_DIR` (default `/tmp/roco_tmux`):

```
/tmp/roco_tmux/
  demo.json                     # session metadata
  demo/
    0/
      0/
        pane.json               # full transcript + ctx memory
```

This mirrors `.roco/sessions/` and `.roco/workspaces/` in the real CLI.

## Test Coverage

The emulator reproduces the assertions from `mock_cli_subcommands` and `mock_tui_interactive`:

```python
from tmux_roco.tmux_emulator import TmuxServer
server = TmuxServer()
sess = server.new_session("test", domain="chat")
pane = sess.get_window("0").get_pane("0")
out, ok = pane.send("Once upon a time")
assert "MOCK_INFERENCE_RESULT" in out
assert ok
assert pane.verifier.verify(out)
assert pane.sandbox.allowed("test.txt")
assert not pane.sandbox.allowed("test.exe")
from tmux_roco.sandbox import is_safe_relative_path
assert is_safe_relative_path("foo/bar.txt")
assert not is_safe_relative_path("../escaped.txt")
```

## Environment Variables (same as Rust)

```
ROCO_USE_MOCK_BACKEND=1
RWKV_MODEL=mock-model
ROCO_TMUX_DIR=/tmp/roco_tmux
```

## Next Steps

- When cargo is available: `cargo build -p roco-cli` then `ROCO_USE_MOCK_BACKEND=1 target/debug/roco --mock interact`
- With real tmux: `tmux new-session -s roco` described above
- In offline: `python3 scripts/tmux_roco.py attach -t roco`

