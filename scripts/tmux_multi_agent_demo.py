#!/usr/bin/env python3
"""
tmux_multi_agent_demo.py — parallel mocked agents via tmux emulator (threads = panes)

Simulates 3 tmux panes/windows running different RoCo domains concurrently,
mirroring how you'd use real tmux:

  tmux new-session -d -s parallel -n chat
  tmux new-window -t parallel -n coding
  tmux new-window -t parallel -n writing
  tmux send-keys -t parallel:0.0 "prompt" Enter
  tmux send-keys -t parallel:1.0 "prompt" Enter
  tmux send-keys -t parallel:2.0 "prompt" Enter
  tmux capture-pane -t parallel:0.0 -p

Each pane is backed by crates/harness mirrored logic (Domains, ExecutionLoop, Verifier, Sandbox).
"""
import sys
from pathlib import Path
import threading
import time

ROOT = Path(__file__).parent
sys.path.insert(0, str(ROOT))
# Make tmux_roco package importable
sys.path.insert(0, str(ROOT))

from tmux_roco.tmux_emulator import TmuxServer
from tmux_roco.domains import StackRunner, list_domains, total_use_cases
from tmux_roco.sandbox import is_safe_relative_path

print("=== RoCo Mocked Multi-Agent tmux Demo ===\n")
print(f"Domains: {list_domains()}")
print(f"Total use cases: {total_use_cases()}\n")

server = TmuxServer()
# clean old parallel if exists
server.kill_session("parallel")

sess = server.new_session("parallel", domain="chat")
print(f"Created session {sess.name}")

# create windows for coding and writing (chat already exists as window 0)
win_chat = sess.get_window("0")
win_coding = sess.new_window(domain="coding")
win_writing = sess.new_window(domain="writing")
# persist newly created windows immediately
server._save(sess.name)

print(f"Windows: {len(sess.windows)}")
for w in sess.list_windows():
    p = w.get_pane("0")
    print(f"  {sess.name}:{w.window_id} -> pane {p.target} domain={p.domain}")

# Prompts per domain (like scripted TUI)
prompts = {
    "chat": "Once upon a time in a kingdom far away, there was a lighthouse keeper",
    "coding": "How do I parse JSON in Rust with serde?",
    "writing": "A clockmaker builds a device that freezes time — write outline",
}

# Threaded send = parallel panes
results = {}

def run_pane(window_id: str, domain: str, prompt: str):
    target = f"parallel:{window_id}.0"
    pane = server.resolve_target(target)
    if not pane:
        print(f"  [WARN] can't resolve {target}")
        return
    print(f"\n[{target} {domain}] sending: {prompt[:60]}...")
    t0 = time.time()
    out, ok = pane.send(prompt, use_loop=False)
    dt = time.time() - t0
    # also run loop version
    out_loop, ok_loop = pane.send(prompt, use_loop=True)
    # persist after send
    try:
        server._save("parallel")
    except Exception:
        pass
    results[target] = {
        "domain": domain,
        "prompt": prompt,
        "out": out,
        "ok": ok,
        "out_loop": out_loop,
        "ok_loop": ok_loop,
        "dt": dt,
        "history_len": len(pane.history),
    }
    print(f"  [{target}] ok={ok} loop_ok={ok_loop} dt={dt:.3f}s history={len(pane.history)}")
    print(f"  -> {out[:120]}")

threads = []
for win_id, domain in [("0", "chat"), ("1", "coding"), ("2", "writing")]:
    t = threading.Thread(target=run_pane, args=(win_id, domain, prompts[domain]))
    threads.append(t)
    t.start()

for t in threads:
    t.join()

print("\n=== Captured Panes ===\n")
for target in ["parallel:0.0", "parallel:1.0", "parallel:2.0"]:
    content = server.cmd_capture_pane(target)
    print(f"--- {target} ---")
    # last 5 lines
    lines = content.splitlines()[-6:]
    for l in lines:
        print(l)
    print()

print("=== Full Stack Runner (coding) ===")
res = StackRunner.run_all("A detective arrives at a dark alley in rain")
print(f"  success={res.success} attempts={res.attempts} rollback={res.rollback_count}")
print(f"  output={res.output[:200]}")

print("\n=== Sandbox Safety Checks (mirrors Rust tests) ===")
checks = [
    ("foo/bar.txt", True),
    ("code.rs", True),
    ("../escaped.txt", False),
    ("./local.txt", False),
    ("/etc/passwd", False),
]
for path, expected in checks:
    got = is_safe_relative_path(path)
    status = "✓" if got == expected else "✗"
    print(f"  {status} is_safe_relative_path({path!r})={got} expected={expected}")

print("\n=== Session List (like tmux ls) ===")
print(server.cmd_list_sessions())
print(server.cmd_list_windows("parallel"))
print(server.cmd_list_panes("parallel:0"))
print(server.cmd_list_panes("parallel:1"))
print(server.cmd_list_panes("parallel:2"))

print("\n=== Results Summary ===")
for tgt, r in results.items():
    print(f"{tgt} [{r['domain']}] ok={r['ok']} loop_ok={r['ok_loop']} hist={r['history_len']} dt={r['dt']:.2f}s")

print("\nDemo complete. Persisted in", server.persist_dir)
print("To interact: python3 scripts/tmux_roco.py attach -t parallel")
print("To clean: python3 scripts/tmux_roco.py kill-session -t parallel")
