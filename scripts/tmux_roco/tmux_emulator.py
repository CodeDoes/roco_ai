"""
Tmux Emulator — Python implementation of tmux-like session/window/pane management
for interacting with mocked RoCo agents when real tmux binary is unavailable.

It provides a similar CLI surface to real tmux so scripts written for real tmux
continue to work with the emulator fallback.

Features:
 - Server holds Sessions
 - Session holds Windows (indexed)
 - Window holds Panes (indexed)
 - Each Pane holds:
    * an Agent (DomainHarness)
    * history buffer (like tmux pane scrollback)
    * backend state save/load
    * sandbox (optional isolated workspace)

Real tmux interop:
 - If `tmux` binary exists, TmuxEmulator can delegate to real tmux via subprocess
   while still using MockBackend inside panes.

Persistence:
 - Sessions saved to /tmp/roco_tmux/ or $ROCO_TMUX_DIR as JSON + log files
 - Compatible with both ephemeral and durable workflows

API mimics tmux commands:
  new-session -s NAME [-d] [domain]
  list-sessions
  kill-session -t NAME
  list-windows -t SESSION
  new-window -t SESSION[:WINDOW] [domain]
  list-panes -t SESSION:WINDOW
  send-keys -t SESSION:WINDOW.PANE "text" [Enter]
  capture-pane -t TARGET [-p] (print buffer)
  attach -t TARGET (interactive REPL)
"""
from __future__ import annotations
import os
import sys
import json
import time
import uuid
import shutil
import threading
from pathlib import Path
from dataclasses import dataclass, field, asdict
from typing import Dict, List, Optional, Tuple
import subprocess

from .framework import Context, State, HarnessConfig, ExecutionLoop
from .domains import create_agent, list_domains, DOMAIN_REGISTRY
from .verifier import Verifier
from .sandbox import Sandbox
from .mock_backend import MockBackend

# ---------- Pane ----------

@dataclass
class PaneLogEntry:
    ts: float
    role: str  # "user", "assistant", "system", "tool"
    content: str
    domain: str
    success: bool = True
    attempts: int = 1

    def render(self) -> str:
        t = time.strftime("%H:%M:%S", time.localtime(self.ts))
        sym = {"user": "💬", "assistant": "🤖", "system": "⚙️", "tool": "🔧"}.get(self.role, "•")
        return f"[{t}] {sym} {self.role}@{self.domain}: {self.content}"


class TmuxPane:
    def __init__(self, pane_id: str, domain: str = "chat", window_id: str = "0", session_name: str = "roco", workspace_root: Optional[Path] = None):
        self.pane_id = pane_id
        self.index = int(pane_id.split(".")[-1]) if "." in pane_id else 0
        self.domain = domain
        self.window_id = window_id
        self.session_name = session_name
        self.target = f"{session_name}:{window_id}.{self.index}"
        self.agent = create_agent(domain)
        self.agent.init(HarnessConfig())
        self.history: List[PaneLogEntry] = []
        self.ctx = Context(session_id=f"{session_name}_{window_id}_{self.index}_{uuid.uuid4().hex[:6]}")
        self.verifier = Verifier()
        self.backend = MockBackend(name=f"{session_name}_{domain}")
        self.sandbox = Sandbox(workspace_root or Path(f"/tmp/roco_tmux/{session_name}/{window_id}/{self.index}"))
        self.sandbox.root.mkdir(parents=True, exist_ok=True)
        self.loop = ExecutionLoop(max_attempts=3)
        self._lock = threading.Lock()
        self.system_log(f"pane {self.target} initialized domain={domain}")

    def system_log(self, msg: str):
        self.history.append(PaneLogEntry(ts=time.time(), role="system", content=msg, domain=self.domain))

    def switch_domain(self, new_domain: str) -> str:
        with self._lock:
            old = self.domain
            self.domain = new_domain
            self.agent = create_agent(new_domain)
            self.agent.init(HarnessConfig())
            self.backend = MockBackend(name=f"{self.session_name}_{new_domain}")
            msg = f"switched domain {old} -> {new_domain}"
            self.system_log(msg)
            return msg

    def send(self, input_text: str, use_loop: bool = False) -> Tuple[str, bool]:
        """Send user input to agent, return (output, success)."""
        with self._lock:
            # record user
            self.history.append(PaneLogEntry(ts=time.time(), role="user", content=input_text, domain=self.domain))
            # context memory
            self.ctx.memory.append(input_text)
            if len(self.ctx.memory) > 20:
                self.ctx.memory = self.ctx.memory[-20:]

            if use_loop:
                res = self.loop.execute(self.agent, input_text, self.ctx)
                out = res.output
                success = res.success
                self.history.append(PaneLogEntry(ts=time.time(), role="assistant", content=out, domain=self.domain, success=success, attempts=res.attempts))
                if not success:
                    self.system_log(f"loop failed attempts={res.attempts} rollback_count={res.rollback_count}")
                return out, success
            else:
                try:
                    out = self.agent.run(input_text, self.ctx)
                    success = self.agent.verify(out)
                    if not success:
                        # trigger rollback logic same as Rust loop
                        st = self.agent.rollback(State())
                        self.system_log(f"verify failed, rollback attempts={st.attempts} checkpoint={st.checkpoint}")
                    self.history.append(PaneLogEntry(ts=time.time(), role="assistant", content=out, domain=self.domain, success=success))
                    return out, success
                except Exception as e:
                    err = f"[Error: {e}]"
                    self.history.append(PaneLogEntry(ts=time.time(), role="assistant", content=err, domain=self.domain, success=False))
                    return err, False

    def capture(self, lines: int = -1) -> str:
        with self._lock:
            entries = self.history[-lines:] if lines > 0 else self.history
            return "\n".join(e.render() for e in entries)

    def clear(self):
        with self._lock:
            self.history.clear()
            self.system_log("history cleared")

    def save_session(self, path: Path):
        data = {
            "pane_id": self.pane_id,
            "target": self.target,
            "domain": self.domain,
            "history": [asdict(h) for h in self.history],
            "ctx": {"session_id": self.ctx.session_id, "memory": self.ctx.memory, "tool_results": self.ctx.tool_results},
        }
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    def load_session(self, path: Path):
        if not path.exists():
            return
        data = json.loads(path.read_text())
        self.domain = data.get("domain", self.domain)
        self.agent = create_agent(self.domain)
        self.agent.init(HarnessConfig())
        hist = []
        for h in data.get("history", []):
            hist.append(PaneLogEntry(**h))
        self.history = hist
        ctxd = data.get("ctx", {})
        self.ctx.session_id = ctxd.get("session_id", self.ctx.session_id)
        self.ctx.memory = ctxd.get("memory", [])
        self.ctx.tool_results = ctxd.get("tool_results", {})


# ---------- Window ----------

class TmuxWindow:
    def __init__(self, window_id: str, session_name: str, initial_domain: str = "chat"):
        self.window_id = window_id
        self.session_name = session_name
        self.index = int(window_id)
        self.panes: Dict[str, TmuxPane] = {}
        self.active_pane = "0"
        # create initial pane
        self.new_pane(domain=initial_domain)

    def new_pane(self, domain: str = "chat") -> TmuxPane:
        pane_idx = str(len(self.panes))
        pane_id = f"{self.window_id}.{pane_idx}"
        pane = TmuxPane(pane_id=pane_id, domain=domain, window_id=self.window_id, session_name=self.session_name)
        self.panes[pane_idx] = pane
        self.active_pane = pane_idx
        return pane

    def get_pane(self, pane_idx: str = "0") -> Optional[TmuxPane]:
        return self.panes.get(pane_idx)

    def list_panes(self) -> List[TmuxPane]:
        return list(self.panes.values())

    def target(self) -> str:
        return f"{self.session_name}:{self.window_id}"


# ---------- Session ----------

class TmuxSession:
    def __init__(self, name: str, initial_domain: str = "chat"):
        self.name = name
        self.created = time.time()
        self.windows: Dict[str, TmuxWindow] = {}
        self.active_window = "0"
        self.new_window(domain=initial_domain)

    def new_window(self, domain: str = "chat") -> TmuxWindow:
        win_idx = str(len(self.windows))
        win = TmuxWindow(window_id=win_idx, session_name=self.name, initial_domain=domain)
        self.windows[win_idx] = win
        self.active_window = win_idx
        return win

    def get_window(self, win_idx: str = "0") -> Optional[TmuxWindow]:
        return self.windows.get(win_idx)

    def list_windows(self) -> List[TmuxWindow]:
        return list(self.windows.values())

    def parse_target(self, target: str) -> Tuple[str, str, str]:
        """
        Parse tmux target TARGETsyntax:
          session:window.pane
          session:window
          session
          :window.pane etc
        Returns (session, window, pane)
        """
        # default to this session if target omits session
        sess = self.name
        win = self.active_window
        pane = "0"

        if ":" in target:
            left, right = target.split(":", 1)
            if left:
                sess = left
            if "." in right:
                w, p = right.split(".", 1)
                if w:
                    win = w
                if p:
                    pane = p
            else:
                if right:
                    win = right
        else:
            # no colon, could be session only or window.pane
            if "." in target:
                w, p = target.split(".", 1)
                if w:
                    win = w
                pane = p
            elif target:
                # if target looks numeric, treat as window, else session
                if target.isdigit():
                    win = target
                else:
                    sess = target

        return sess, win, pane

    def get_pane_by_target(self, target: str) -> Optional[TmuxPane]:
        # target parsing within session — if target includes session, ignore for now (server resolves)
        _, win_idx, pane_idx = self.parse_target(target)
        win = self.get_window(win_idx)
        if not win:
            return None
        return win.get_pane(pane_idx)


# ---------- Server (owns all sessions) ----------

class TmuxServer:
    def __init__(self, persist_dir: Optional[Path] = None):
        self.persist_dir = persist_dir or Path(os.environ.get("ROCO_TMUX_DIR", "/tmp/roco_tmux"))
        self.persist_dir.mkdir(parents=True, exist_ok=True)
        self.sessions: Dict[str, TmuxSession] = {}
        self._lock = threading.Lock()
        # Detect real tmux vs fake emulator wrapper
        real_available = False
        tmux_path = shutil.which("tmux")
        if tmux_path:
            # Prevent recursion: if tmux binary is our fake wrapper, treat as not real
            # Also respect env var FAKE_TMUX or TMUX_EMULATOR
            if os.environ.get("FAKE_TMUX") or os.environ.get("TMUX_EMULATOR"):
                real_available = False
            else:
                try:
                    # Quick check: run `tmux -V` and look for emulator marker
                    proc = subprocess.run([tmux_path, "-V"], capture_output=True, text=True, timeout=2)
                    out = (proc.stdout + proc.stderr).lower()
                    if "emulator" in out:
                        real_available = False
                    else:
                        # Also check if binary is a shell script (fake)
                        # Real tmux is ELF, fake is text
                        if tmux_path.startswith(str(Path.home())):
                            # likely our fake at ~/.local/bin/tmux — check file magic
                            try:
                                with open(tmux_path, 'rb') as f:
                                    magic = f.read(4)
                                if magic.startswith(b'#!'):
                                    real_available = False
                                else:
                                    real_available = True
                            except:
                                real_available = False
                        else:
                            real_available = True
                except Exception:
                    real_available = False
        self.real_tmux_available = real_available
        self._load_persisted()

    def _persist_file(self, sess_name: str) -> Path:
        return self.persist_dir / f"{sess_name}.json"

    def _load_persisted(self):
        if not self.persist_dir.exists():
            return
        for jf in self.persist_dir.glob("*.json"):
            if jf.parent != self.persist_dir:
                continue
            try:
                data = json.loads(jf.read_text())
                name = data.get("name") or jf.stem
                # avoid loading full history files which are nested deeper? we only glob top level
                if name.endswith("_full") or "/" in name:
                    continue
                if name not in self.sessions:
                    sess = TmuxSession(name=name, initial_domain=data.get("active_domain", "chat"))
                    # try to load pane histories
                    try:
                        self._load_pane_histories(sess)
                    except Exception:
                        pass
                    self.sessions[name] = sess
            except Exception:
                continue
        # Also load sessions that only have pane folders but no json (e.g., after first save)
        for d in self.persist_dir.iterdir():
            if d.is_dir():
                sess_name = d.name
                if sess_name not in self.sessions:
                    sess = TmuxSession(name=sess_name, initial_domain="chat")
                    try:
                        self._load_pane_histories(sess)
                    except Exception:
                        pass
                    self.sessions[sess_name] = sess

    def new_session(self, name: str, domain: str = "chat", detach: bool = False) -> TmuxSession:
        with self._lock:
            if name in self.sessions:
                raise ValueError(f"session {name} already exists")
            sess = TmuxSession(name=name, initial_domain=domain)
            self.sessions[name] = sess
            self._save(name)
            return sess

    def get_session(self, name: str) -> Optional[TmuxSession]:
        return self.sessions.get(name)

    def list_sessions(self) -> List[TmuxSession]:
        return list(self.sessions.values())

    def kill_session(self, name: str) -> bool:
        with self._lock:
            if name in self.sessions:
                del self.sessions[name]
                pf = self._persist_file(name)
                if pf.exists():
                    pf.unlink()
                # also clean dir
                d = self.persist_dir / name
                if d.exists():
                    shutil.rmtree(d, ignore_errors=True)
                return True
            return False

    def _pane_history_path(self, sess_name: str, win_id: str, pane_idx: str) -> Path:
        return self.persist_dir / sess_name / win_id / pane_idx / "pane.json"

    def _save(self, sess_name: str):
        sess = self.sessions.get(sess_name)
        if not sess:
            return
        try:
            # session metadata
            data = {
                "name": sess.name,
                "created": sess.created,
                "active_window": sess.active_window,
                "windows": len(sess.windows),
                "active_domain": sess.get_window(sess.active_window).get_pane("0").domain if sess.get_window(sess.active_window) else "chat",
            }
            self._persist_file(sess_name).write_text(json.dumps(data, indent=2))
            # full pane histories under session folder
            for win_id, win in sess.windows.items():
                for pane_idx, pane in win.panes.items():
                    ppath = self._pane_history_path(sess_name, win_id, pane_idx)
                    try:
                        pane.save_session(ppath)
                    except Exception:
                        pass
        except Exception:
            pass

    def _load_pane_histories(self, sess: TmuxSession):
        # attempt to load pane histories from disk if they exist
        base = self.persist_dir / sess.name
        if not base.exists():
            return
        for win_dir in base.iterdir():
            if not win_dir.is_dir():
                continue
            win_id = win_dir.name
            # ensure window exists (create missing)
            win = sess.get_window(win_id)
            if not win:
                # create window if not exists
                win = sess.new_window(domain="chat")
                # override its id? We'll keep as is; ensure mapping
                # Actually new_window auto indexes, so we may need to adjust
                # For simplicity, if win_id not present, we repurpose last window's id
                # Better: if len mismatch, we just load what we can
                pass
            for pane_dir in win_dir.iterdir():
                if not pane_dir.is_dir():
                    continue
                pane_idx = pane_dir.name
                pane_file = pane_dir / "pane.json"
                if not pane_file.exists():
                    continue
                # ensure pane exists
                win_obj = sess.get_window(win_id)
                if not win_obj:
                    continue
                pane = win_obj.get_pane(pane_idx)
                if not pane:
                    # create new pane if missing
                    pane = win_obj.new_pane(domain="chat")
                    # fix index if needed — but we will load anyway
                try:
                    pane.load_session(pane_file)
                except Exception:
                    pass

    def resolve_target(self, target: str) -> Optional[TmuxPane]:
        """
        Resolve full target session:window.pane to Pane.
        target may be empty -> active session's active pane.
        """
        if not target:
            # pick first session's active
            if not self.sessions:
                return None
            sess = list(self.sessions.values())[0]
            win = sess.get_window(sess.active_window)
            if not win:
                return None
            return win.get_pane(win.active_pane)

        # parse session part
        sess_name = target.split(":")[0] if ":" in target else (target if not target[0].isdigit() and "." not in target and ":" not in target else None)
        # If target like "0.1" without session, use first session
        if ":" not in target and sess_name and (sess_name.isdigit() or "." in sess_name):
            sess_name = None

        if sess_name:
            # target includes session
            # split
            if ":" in target:
                sess_part, rest = target.split(":", 1)
                sess = self.get_session(sess_part)
                if not sess:
                    return None
                # rest is window.pane
                win_idx = "0"
                pane_idx = "0"
                if "." in rest:
                    win_idx, pane_idx = rest.split(".", 1)
                    win_idx = win_idx or sess.active_window
                else:
                    win_idx = rest or sess.active_window
                win = sess.get_window(win_idx)
                if not win:
                    return None
                return win.get_pane(pane_idx)
            else:
                # just session name
                sess = self.get_session(target)
                if not sess:
                    return None
                win = sess.get_window(sess.active_window)
                if not win:
                    return None
                return win.get_pane(win.active_pane)
        else:
            # no session prefix, use first session
            if not self.sessions:
                return None
            sess = list(self.sessions.values())[0]
            # target may be window.pane
            win_idx = sess.active_window
            pane_idx = "0"
            if "." in target:
                w, p = target.split(".", 1)
                win_idx = w or win_idx
                pane_idx = p
            elif target.isdigit():
                win_idx = target
            win = sess.get_window(win_idx)
            if not win:
                return None
            return win.get_pane(pane_idx)

    # ----- emulation of tmux CLI commands -----
    def cmd_new_session(self, name: str, domain: str = "chat", detach: bool = False) -> str:
        try:
            self.new_session(name, domain=domain, detach=detach)
            return f"[emulator] new session {name} domain={domain}"
        except ValueError as e:
            return str(e)

    def cmd_list_sessions(self) -> str:
        if not self.sessions:
            return "[emulator] no sessions"
        lines = []
        for s in self.sessions.values():
            age = int(time.time() - s.created)
            wins = len(s.windows)
            pane = s.get_window(s.active_window).get_pane("0") if s.get_window(s.active_window) else None
            dom = pane.domain if pane else "?"
            lines.append(f"{s.name}: {wins} windows (created {age}s ago) [active domain={dom}]")
        return "\n".join(lines)

    def cmd_send_keys(self, target: str, text: str, use_loop: bool = False) -> str:
        pane = self.resolve_target(target)
        if not pane:
            return f"can't find pane {target}"
        out, ok = pane.send(text, use_loop=use_loop)
        self._save(pane.session_name)
        status = "✓" if ok else "✗"
        return f"{status} {pane.target} ({pane.domain}) -> {out[:500]}"

    def cmd_capture_pane(self, target: str, lines: int = -1) -> str:
        pane = self.resolve_target(target)
        if not pane:
            return f"can't find pane {target}"
        return pane.capture(lines=lines)

    def cmd_list_windows(self, session: str) -> str:
        sess = self.get_session(session)
        if not sess:
            return f"no session {session}"
        out = []
        for w in sess.list_windows():
            panes = len(w.panes)
            active_dom = w.get_pane(w.active_pane).domain if w.get_pane(w.active_pane) else "?"
            out.append(f"{sess.name}:{w.window_id}: {panes} panes [active {active_dom}] (active)")
        return "\n".join(out)

    def cmd_list_panes(self, target: str) -> str:
        # target is session:window
        sess_name = target.split(":")[0] if ":" in target else target
        win_idx = "0"
        if ":" in target:
            win_idx = target.split(":")[1] or "0"
        sess = self.get_session(sess_name)
        if not sess:
            return f"no session {sess_name}"
        win = sess.get_window(win_idx)
        if not win:
            return f"no window {win_idx} in {sess_name}"
        lines = []
        for p in win.list_panes():
            lines.append(f"{sess.name}:{win.window_id}.{p.index}: [{p.domain}] {len(p.history)} lines history")
        return "\n".join(lines)

    # Real tmux delegation (if binary exists)
    def real_tmux_cmd(self, *args) -> str:
        if not self.real_tmux_available:
            return "[emulator] real tmux not available, using emulator"
        try:
            proc = subprocess.run(["tmux"] + list(args), capture_output=True, text=True, timeout=5)
            return proc.stdout + proc.stderr
        except Exception as e:
            return f"tmux error: {e}"
