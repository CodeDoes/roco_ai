"""
MockBackend — deterministic Python mirror of roco_engine::MockBackend and
roco_harness::framework::MockBackend.

In Rust:
  MockBackend.generate(prompt) -> "MOCK_INFERENCE_RESULT: {prompt.trim()}"

This file adds persistence-friendly helpers used by tmux panes.
"""
from __future__ import annotations
import json
import time
from dataclasses import dataclass, field
from typing import Optional, Dict, Any


@dataclass
class CompletionResponse:
    text: str
    parsed: Optional[Dict[str, Any]] = None
    think_trace: Optional[str] = None

    def to_dict(self):
        return {"text": self.text, "parsed": self.parsed, "think_trace": self.think_trace}


class MockBackend:
    """Deterministic mock, identical contract to Rust version."""

    def __init__(self, name: str = "mock"):
        self.name = name
        self._state_counter = 0
        self._last_prompt = ""

    def generate(self, prompt: str) -> str:
        """Core generation — matches Rust exactly."""
        prompt = prompt.strip()
        self._last_prompt = prompt
        self._state_counter += 1
        return f"MOCK_INFERENCE_RESULT: {prompt}"

    def detailed_run(self, input_str: str) -> str:
        """Matches the `detailed_run` helper present in every domain."""
        return f"[MOCK_DETAILED_{input_str}] input_length={len(input_str)} output_generated"

    async def complete(self, system: str = "", prompt: str = "", thinking: bool = False) -> CompletionResponse:
        # Sync version wrapped for async compatibility
        txt = self.generate(f"{system} {prompt}".strip() if system else prompt)
        parsed = {"mock": True, "len": len(txt), "counter": self._state_counter}
        trace = None
        if thinking:
            trace = f"thinking trace for: {prompt[:40]}"
            txt = f" thinking {trace} response {txt}"
        return CompletionResponse(text=txt, parsed=parsed, think_trace=trace)

    def complete_sync(self, system: str = "", prompt: str = "", thinking: bool = False) -> CompletionResponse:
        txt = self.generate(f"{system} {prompt}".strip() if system else prompt)
        parsed = {"mock": True, "len": len(txt), "counter": self._state_counter}
        trace = None
        if thinking:
            trace = f"thinking trace for: {prompt[:40]}"
            txt = f" thinking {trace} response {txt}"
        return CompletionResponse(text=txt, parsed=parsed, think_trace=trace)

    def save_state(self) -> bytes:
        payload = {
            "backend": self.name,
            "counter": self._state_counter,
            "last_prompt": self._last_prompt,
            "ts": time.time(),
            "valid_mock_state": True,
        }
        return json.dumps(payload).encode("utf-8")

    def load_state(self, data: bytes) -> None:
        try:
            obj = json.loads(data.decode("utf-8"))
            if not obj.get("valid_mock_state"):
                raise ValueError("invalid mock state")
            self._state_counter = obj.get("counter", 0)
            self._last_prompt = obj.get("last_prompt", "")
        except json.JSONDecodeError as e:
            raise ValueError(f"invalid mock state: {e}")
        except Exception as e:
            if "invalid mock state" in str(e):
                raise
            raise ValueError(f"invalid mock state: {e}")

    def mix_states(self, a: bytes, b: bytes, ratio: float) -> bytes:
        ja = json.loads(a.decode())
        jb = json.loads(b.decode())
        mixed = {
            "mixed_ratio": ratio,
            "source_a": ja,
            "source_b": jb,
            "valid_mock_state": True,
            "ts": time.time(),
        }
        return json.dumps(mixed).encode()

    def feed_eos(self) -> None:
        self._state_counter += 1

    def interrupt(self) -> None:
        pass
