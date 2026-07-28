"""
Sandbox — Python mirror of crates/harness/src/sandbox.rs
Enforces file access boundaries with strict containment checks.
"""
from __future__ import annotations
import os
from pathlib import Path, PurePath
from typing import List

ALLOWED_EXTS = ["txt", "md", "json", "py", "rs"]
MAX_FILE_SIZE = 10_000_000


def is_safe_relative_path(path_str: str) -> bool:
    """Mirrors Rust Sandbox::is_safe_relative_path"""
    if not path_str:
        return False
    p = Path(path_str)
    # absolute?
    if p.is_absolute():
        return False
    # Reject any component that is parent, cur, root, prefix
    # Python pathlib split
    parts = PurePath(path_str).parts
    for part in parts:
        if part in ("..", ".", "/", "\\"):
            return False
        # Also check for parent traversal embedded
        if ".." in part or part.startswith("/"):
            # need stricter: if path contains ".." as component
            pass
    # Use explicit check for .. and .
    # Path.parts already normalized? Let's check raw string
    if ".." in Path(path_str).parts:
        return False
    if "." in Path(path_str).parts:
        return False
    # Check for absolute or prefix-like (e.g., C:\\)
    if path_str.startswith("/") or path_str.startswith("\\"):
        return False
    if ":" in path_str.split("/")[0] and len(path_str.split("/")[0]) <= 3:
        # heuristic for Windows prefix
        return False
    # Disallow '.' alone
    if any(seg == "." or seg == ".." for seg in path_str.split("/")):
        return False
    # Also reject if normalized escapes
    # Must be relative and not contain .. 
    normalized = os.path.normpath(path_str)
    if normalized.startswith("..") or os.path.isabs(normalized):
        return False
    # prevent "./local.txt" case - contains . component
    for seg in normalized.split(os.sep):
        if seg == "." or seg == "..":
            return False
    return True


class Sandbox:
    def __init__(self, root: str | Path):
        self.root = Path(root).resolve()
        self.allowed_exts = ALLOWED_EXTS[:]

    def allowed(self, path: str) -> bool:
        return any(path.endswith(ext) for ext in self.allowed_exts)

    def _full_path(self, path: str) -> Path:
        return (self.root / path).resolve()

    def _check_escape(self, path: str) -> None:
        if not is_safe_relative_path(path):
            raise ValueError("path escape blocked")
        full = self.root / path
        # Use resolve to check containment - but we must ensure root is parent of full's parent that exists?
        # For safety, check that normalized full starts with root
        try:
            resolved = full.resolve()
        except:
            resolved = (self.root / path).absolute()
        # Ensure resolved is inside root (allow root itself)
        try:
            resolved.relative_to(self.root)
        except ValueError:
            # also check lexical starts_with for simple containment before file exists
            if not str(resolved).startswith(str(self.root)):
                raise ValueError("path escape blocked")

    def read(self, path: str) -> str:
        if not is_safe_relative_path(path):
            raise ValueError("path escape blocked")
        if not self.allowed(path):
            raise ValueError("file extension not allowed")
        full = self.root / path
        # containment check
        if not str(full.resolve()).startswith(str(self.root)):
            # for non-existing files, check lexical
            if not str((self.root / path).absolute()).startswith(str(self.root)):
                raise ValueError("path escape blocked")
        if not full.exists():
            raise FileNotFoundError("file not found")
        if full.stat().st_size > MAX_FILE_SIZE:
            raise ValueError("file too large")
        return full.read_text(encoding="utf-8", errors="ignore")

    def write(self, path: str, content: str) -> None:
        if not is_safe_relative_path(path):
            raise ValueError("path escape blocked")
        if not self.allowed(path):
            raise ValueError("file extension not allowed")
        full = self.root / path
        if not str(full.resolve()).startswith(str(self.root)):
            if not str((self.root / path).absolute()).startswith(str(self.root)):
                raise ValueError("path escape blocked")
        full.parent.mkdir(parents=True, exist_ok=True)
        full.write_text(content, encoding="utf-8")

    def list_files(self) -> List[str]:
        if not self.root.exists():
            return []
        res = []
        for entry in self.root.iterdir():
            res.append(entry.name)
        res.sort()
        return res

    def exists(self, path: str) -> bool:
        return (self.root / path).exists()

    def delete(self, path: str) -> None:
        if not is_safe_relative_path(path):
            raise ValueError("escape")
        full = self.root / path
        if not str(full.resolve()).startswith(str(self.root)):
            raise ValueError("escape")
        full.unlink()

    def size_limit_check(self, path: str, limit: int) -> bool:
        full = self.root / path
        if not full.exists():
            return False
        return full.stat().st_size <= limit
