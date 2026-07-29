"""
CLI — mirrors roco_cli::test_harness and real tmux commands, plus interactive REPL
that lets user interact with mocked RoCo agent via tmux-like panes.

Commands inside attach REPL:
  /help — show help
  /domains — list domains
  /use <domain> — switch domain for current pane
  /windows — list windows
  /panes — list panes
  /new-window [domain] — create window
  /new-pane [domain] — create pane in current window
  /switch <session:window.pane> — switch target
  /target — show current target
  /history [n] — show last n lines
  /clear — clear history
  /loop <text> — run with ExecutionLoop (retry+rollback)
  /verify <text> — run verifier
  /sandbox <read|write|list> <path> [content] — test sandbox
  /save [path] — save pane to file
  /sessions — list sessions
  /quit — exit

Also supports raw tmux-ish invocation from shell:
  tmux_roco.py new-session -s roco -d chat
  tmux_roco.py ls
  tmux_roco.py send-keys -t roco:0.0 "hello" Enter
  tmux_roco.py capture-pane -t roco
  tmux_roco.py attach -t roco
"""
from __future__ import annotations
import os
import sys
import argparse
from pathlib import Path
import time
import shlex

from .tmux_emulator import TmuxServer, TmuxSession, TmuxPane
from .domains import list_domains, list_use_cases, total_use_cases, create_agent
from .verifier import Verifier
from .sandbox import Sandbox, is_safe_relative_path
from .mock_backend import MockBackend


SERVER = TmuxServer()

BANNER = r"""
██████╗  ██████╗  ██████╗ ██████╗     ███╗   ███╗ ██████╗  ██████╗██╗  ██╗
██╔══██╗██╔═══██╗██╔════╝██╔═══██╗    ████╗ ████║██╔═══██╗██╔════╝██║ ██╔╝
██████╔╝██║   ██║██║     ██║   ██║    ██╔████╔██║██║   ██║██║     █████╔╝ 
██╔══██╗██║   ██║██║     ██║   ██║    ██║╚██╔╝██║██║   ██║██║     ██╔═██╗ 
██║  ██║╚██████╔╝╚██████╗╚██████╔╝    ██║ ╚═╝ ██║╚██████╔╝╚██████╗██║  ██╗
╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═════╝     ╚═╝     ╚═╝ ╚═════╝  ╚═════╝╚═╝  ╚═╝
  Mock Agent via tmux emulator — offline, deterministic, air-gapped
"""

HELP_REPL = """
 RoCo Mock via tmux — Interactive REPL Help
 ─────────────────────────────────────────
  Type any text → sent to current pane's mocked agent

 Slash commands:
  /help                 Show this help
  /domains              List available agent domains (11 + aggregate)
  /use <domain>         Switch domain for current pane
  /windows              List windows in session
  /panes                List panes in current window
  /new-window [domain]  Create new window
  /new-pane [domain]    Create new pane
  /switch <tgt>         Switch target (e.g. roco:1.0, 0.1, coding)
  /target               Show current target
  /history [n]          Show last n history lines (default all)
  /clear                Clear history
  /loop <text>          Run with ExecutionLoop retry+rollback
  /verify <text>        Verify text with Verifier
  /sandbox <op> <path> [content]
                        op=read|write|list|check|safe
  /save [path]          Save pane transcript
  /sessions             List tmux sessions
  /usecases             Show 70 use cases breakdown
  /stack <text>         Run full_stack StackRunner
  /quit, /exit, :q      Exit REPL

 tmux-like shell usage (outside REPL):
  tmux_roco.py new-session -s NAME [-d] [domain]
  tmux_roco.py ls | list-sessions
  tmux_roco.py send-keys -t TARGET "text"
  tmux_roco.py capture-pane -t TARGET
  tmux_roco.py attach -t TARGET
  tmux_roco.py kill-session -t NAME
  tmux_roco.py list-windows -t SESSION
  tmux_roco.py list-panes -t SESSION:WINDOW
"""

def print_banner():
    print(BANNER)
    print(f" Real tmux binary: {'found' if SERVER.real_tmux_available else 'not found — using pure Python emulator'}")
    print(f" Persist dir: {SERVER.persist_dir}")
    print(f" Domains: {', '.join(list_domains())}")
    print(f" Use cases: {total_use_cases()} across {len(list_use_cases())} categories")
    print()

def ensure_session(name: str, domain: str = "chat") -> TmuxSession:
    sess = SERVER.get_session(name)
    if not sess:
        sess = SERVER.new_session(name, domain=domain)
    return sess

def parse_main_args(argv):
    parser = argparse.ArgumentParser(prog="tmux_roco", description="tmux-like interface to mocked RoCo agent")
    sub = parser.add_subparsers(dest="cmd")

    # new-session
    p_new = sub.add_parser("new-session", aliases=["new", "new-sess"])
    p_new.add_argument("-s", "--session", default="roco", help="session name")
    p_new.add_argument("-d", action="store_true", help="detach (don't attach)")
    p_new.add_argument("domain", nargs="?", default="chat", help="initial domain")

    # list-sessions
    sub.add_parser("list-sessions", aliases=["ls", "list", "lss"])

    # kill-session
    p_kill = sub.add_parser("kill-session", aliases=["kill", "ks"])
    p_kill.add_argument("-t", "--target", default="roco", help="session name")

    # send-keys
    p_send = sub.add_parser("send-keys", aliases=["send", "sk"])
    p_send.add_argument("-t", "--target", default="", help="target pane session:window.pane")
    p_send.add_argument("keys", nargs="+", help="text to send")
    p_send.add_argument("--loop", action="store_true", help="use ExecutionLoop")

    # capture-pane
    p_cap = sub.add_parser("capture-pane", aliases=["capture", "cap", "cp"])
    p_cap.add_argument("-t", "--target", default="", help="target pane")
    p_cap.add_argument("-p", action="store_true", help="print (default)")
    p_cap.add_argument("-S", "--start", type=int, default=-1, help="start line (negative = tail)")
    p_cap.add_argument("--lines", type=int, default=-1, help="number lines")

    # attach
    p_att = sub.add_parser("attach", aliases=["att", "a", "interactive", "repl"])
    p_att.add_argument("-t", "--target", default="roco", help="session to attach")
    p_att.add_argument("-d", "--domain", default="chat", help="domain if new session")

    # list-windows
    p_lw = sub.add_parser("list-windows", aliases=["lsw"])
    p_lw.add_argument("-t", "--target", default="roco", help="session name")

    # list-panes
    p_lp = sub.add_parser("list-panes", aliases=["lsp"])
    p_lp.add_argument("-t", "--target", default="roco:0", help="session:window")

    # domains
    sub.add_parser("domains", aliases=["list-domains"])

    # usecases
    sub.add_parser("usecases")

    # run mock CLI (compat with roco_cli::test_harness)
    p_run = sub.add_parser("run-mock", aliases=["roco"])
    p_run.add_argument("roco_args", nargs=argparse.REMAINDER, help="args to pass to mocked roco")

    parser.add_argument("--mock", action="store_true", help="enable mock backend env (compat)")

    if not argv:
        # default to interactive attach
        return parser.parse_args(["attach"])
    return parser.parse_args(argv)

def interactive_repl(session_name: str, initial_domain: str = "chat"):
    print_banner()
    sess = ensure_session(session_name, domain=initial_domain)
    # resolve current pane as active
    win = sess.get_window(sess.active_window)
    pane = win.get_pane(win.active_pane) if win else None
    if not pane:
        win = sess.new_window(domain=initial_domain)
        pane = win.get_pane("0")

    print(f"Attached to session {session_name} — target {pane.target} domain={pane.domain}")
    print("Type /help for commands, /quit to exit, or just type to chat with mocked agent.\n")
    # show short history if any
    if pane.history:
        print(pane.capture(lines=10))
        print()

    current_target = pane.target

    def get_current_pane():
        # re-resolve from server each time (in case switched)
        p = SERVER.resolve_target(current_target)
        return p or pane

    while True:
        try:
            cur_p = get_current_pane()
            prompt = f"\033[1;36m{cur_p.target}\033[0m \033[2m[{cur_p.domain}]\033[0m \033[1m💬 >\033[0m "
            line = input(prompt)
        except EOFError:
            print("\n[EOF — saving and exiting]")
            break
        except KeyboardInterrupt:
            print("\n[Interrupted — type /quit to exit]")
            continue

        if not line.strip():
            continue

        # slash commands
        if line.startswith("/") or line.startswith(":"):
            cmdline = line.lstrip("/:").strip()
            if not cmdline:
                continue
            parts = shlex.split(cmdline) if cmdline else []
            if not parts:
                continue
            cmd = parts[0].lower()
            args = parts[1:]

            if cmd in ("quit", "q", "exit", "stop"):
                print("Goodbye! Session persisted at", SERVER.persist_dir)
                break
            elif cmd == "help" or cmd == "h" or cmd == "?":
                print(HELP_REPL)
            elif cmd == "domains":
                print("Available domains:")
                for d in list_domains():
                    print(f"  - {d}")
            elif cmd == "usecases":
                uc = list_use_cases()
                print(f"Total {total_use_cases()} use cases:")
                for k, v in uc.items():
                    print(f"  {k}: {v}")
            elif cmd == "sessions":
                print(SERVER.cmd_list_sessions())
            elif cmd == "windows":
                sess = SERVER.get_session(session_name)
                if sess:
                    print(SERVER.cmd_list_windows(sess.name))
                else:
                    print("no session")
            elif cmd == "panes":
                cur = get_current_pane()
                tgt = f"{cur.session_name}:{cur.window_id}"
                print(SERVER.cmd_list_panes(tgt))
            elif cmd in ("target", "pane"):
                cur = get_current_pane()
                print(f"current target: {cur.target} domain={cur.domain} history={len(cur.history)}")
            elif cmd == "use":
                if not args:
                    print("usage: /use <domain>")
                    continue
                new_dom = args[0]
                if new_dom not in list_domains():
                    print(f"unknown domain {new_dom}, available: {', '.join(list_domains())}")
                    continue
                cur = get_current_pane()
                msg = cur.switch_domain(new_dom)
                print(msg)
            elif cmd in ("new-window", "neww"):
                dom = args[0] if args else "chat"
                sess = SERVER.get_session(session_name)
                if not sess:
                    sess = ensure_session(session_name, dom)
                win = sess.new_window(domain=dom)
                current_target = win.get_pane("0").target
                print(f"new window {win.target()} domain={dom} -> target now {current_target}")
            elif cmd in ("new-pane", "newp", "split"):
                dom = args[0] if args else get_current_pane().domain
                sess = SERVER.get_session(session_name)
                cur = get_current_pane()
                win = sess.get_window(cur.window_id) if sess else None
                if not win:
                    print("no window")
                    continue
                np = win.new_pane(domain=dom)
                current_target = np.target
                print(f"new pane {np.target} domain={dom}")
            elif cmd == "switch":
                if not args:
                    print("usage: /switch <session:window.pane>")
                    continue
                tgt = args[0]
                p = SERVER.resolve_target(tgt)
                if not p:
                    # try as domain switch if tgt is domain name
                    if tgt in list_domains():
                        cur = get_current_pane()
                        cur.switch_domain(tgt)
                        print(f"switched domain to {tgt}")
                        continue
                    print(f"can't find pane {tgt}")
                    continue
                current_target = p.target
                print(f"switched to {current_target} domain={p.domain}")
            elif cmd == "history":
                n = -1
                if args:
                    try:
                        n = int(args[0])
                    except:
                        n = -1
                cur = get_current_pane()
                print(cur.capture(lines=n))
            elif cmd == "clear":
                cur = get_current_pane()
                cur.clear()
                print("cleared")
            elif cmd == "loop":
                txt = " ".join(args) if args else ""
                if not txt:
                    print("usage: /loop <text>")
                    continue
                cur = get_current_pane()
                out, ok = cur.send(txt, use_loop=True)
                print(f"{'✓' if ok else '✗'} [{cur.domain}] {out}")
            elif cmd == "verify":
                txt = " ".join(args)
                v = Verifier()
                ok = v.verify(txt)
                print(f"{'PASS' if ok else 'FAIL'}: {v.explain(txt)} score={v.score(txt)}")
            elif cmd == "sandbox":
                if len(args) < 1:
                    print("usage: /sandbox <read|write|list|check|safe> <path> [content]")
                    continue
                op = args[0]
                cur = get_current_pane()
                sb = cur.sandbox
                try:
                    if op == "list":
                        print(sb.list_files())
                    elif op == "safe":
                        path = args[1] if len(args) > 1 else ""
                        print(f"{path!r} safe={is_safe_relative_path(path)}")
                    elif op == "read":
                        path = args[1] if len(args) > 1 else ""
                        print(sb.read(path))
                    elif op == "write":
                        if len(args) < 3:
                            print("write needs path and content")
                            continue
                        path = args[1]
                        content = " ".join(args[2:])
                        sb.write(path, content)
                        print(f"wrote {path}")
                    elif op == "check":
                        path = args[1] if len(args) > 1 else ""
                        print(f"allowed={sb.allowed(path)} exists={sb.exists(path)} safe={is_safe_relative_path(path)}")
                    else:
                        print(f"unknown sandbox op {op}")
                except Exception as e:
                    print(f"sandbox error: {e}")
            elif cmd == "save":
                path = Path(args[0]) if args else Path(f"/tmp/roco_tmux/{session_name}_transcript.json")
                cur = get_current_pane()
                cur.save_session(path)
                print(f"saved to {path}")
            elif cmd == "stack":
                txt = " ".join(args) if args else "test full stack"
                from .domains import StackRunner
                res = StackRunner.run_all(txt)
                print(f"StackRunner: success={res.success} attempts={res.attempts} rollback={res.rollback_count}")
                print(f"output: {res.output[:500]}")
            else:
                print(f"unknown command /{cmd} — type /help")
            continue

        # regular chat input → send to current pane agent
        cur = get_current_pane()
        out, ok = cur.send(line, use_loop=False)
        # render with colors
        ok_sym = "\033[32m✓\033[0m" if ok else "\033[31m✗\033[0m"
        print(f"{ok_sym} \033[1;33m🤖 {cur.domain} >\033[0m {out}")

def main(argv=None):
    argv = argv if argv is not None else sys.argv[1:]
    args = parse_main_args(argv)

    if args.cmd in ("new-session", "new", "new-sess"):
        name = args.session
        domain = getattr(args, "domain", "chat")
        detach = getattr(args, "d", False)
        msg = SERVER.cmd_new_session(name, domain=domain, detach=detach)
        print(msg)
        if not detach:
            interactive_repl(name, domain)
        return

    if args.cmd in ("list-sessions", "ls", "list", "lss"):
        print(SERVER.cmd_list_sessions())
        # also try real tmux
        if SERVER.real_tmux_available:
            print("\n--- real tmux sessions ---")
            print(SERVER.real_tmux_cmd("ls"))
        return

    if args.cmd in ("kill-session", "kill", "ks"):
        tgt = getattr(args, "target", "roco")
        ok = SERVER.kill_session(tgt)
        print(f"killed {tgt}: {ok}")
        return

    if args.cmd in ("send-keys", "send", "sk"):
        tgt = getattr(args, "target", "")
        keys = getattr(args, "keys", [])
        # tmux convention last arg may be Enter — ignore
        txt = " ".join(k for k in keys if k != "Enter")
        use_loop = getattr(args, "loop", False)
        print(SERVER.cmd_send_keys(tgt, txt, use_loop=use_loop))
        return

    if args.cmd in ("capture-pane", "capture", "cap", "cp"):
        tgt = getattr(args, "target", "")
        lines = getattr(args, "lines", -1)
        if getattr(args, "start", -1) != -1:
            lines = getattr(args, "start", -1)
        print(SERVER.cmd_capture_pane(tgt, lines=lines))
        return

    if args.cmd in ("list-windows", "lsw"):
        tgt = getattr(args, "target", "roco")
        print(SERVER.cmd_list_windows(tgt))
        return

    if args.cmd in ("list-panes", "lsp"):
        tgt = getattr(args, "target", "roco:0")
        print(SERVER.cmd_list_panes(tgt))
        return

    if args.cmd in ("domains", "list-domains"):
        for d in list_domains():
            print(d)
        return

    if args.cmd == "usecases":
        uc = list_use_cases()
        for k, v in uc.items():
            print(f"{k}: {v}")
        print(f"total: {total_use_cases()}")
        return

    if args.cmd in ("attach", "att", "a", "interactive", "repl", None):
        tgt = getattr(args, "target", "roco") if hasattr(args, "target") else "roco"
        dom = getattr(args, "domain", "chat") if hasattr(args, "domain") else "chat"
        # tgt could be session:window.pane — extract session
        sess_name = tgt.split(":")[0] if ":" in tgt else tgt
        if not sess_name:
            sess_name = "roco"
        interactive_repl(sess_name, dom)
        return

    if args.cmd in ("run-mock", "roco"):
        # Emulate roco CLI mock run
        roco_args = getattr(args, "roco_args", [])
        # if first arg starts with --mock strip
        print("[mock roco-cli] args:", roco_args)
        # simple dispatch: if args contain interact, start repl
        if not roco_args:
            interactive_repl("roco", "chat")
            return
        # handle interact, code, game, etc as domains
        mapping = {
            "interact": "chat",
            "code": "coding",
            "coder": "coding",
            "game": "debug",
            "router": "chat",
            "story": "writing",
            "story-mode": "writing",
            "html": "html",
            "browser": "browser",
        }
        sub = roco_args[0] if roco_args else "chat"
        dom = mapping.get(sub, sub if sub in list_domains() else "chat")
        prompt = " ".join(roco_args[1:]) if len(roco_args) > 1 else "hello"
        if dom not in list_domains():
            dom = "chat"
            prompt = " ".join(roco_args)
        agent = create_agent(dom)
        from .framework import Context
        ctx = Context(session_id="mock_cli")
        out = agent.run(prompt, ctx)
        print(out)
        return

    print(f"unknown command {args.cmd}")
    print(HELP_REPL)

if __name__ == "__main__":
    main()
