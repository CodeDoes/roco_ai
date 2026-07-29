"""
Framework — Python mirror of crates/harness/src/framework.rs

DomainHarness trait, HarnessConfig, Context, State, HarnessError, ExecutionLoop
"""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple
from abc import ABC, abstractmethod
import time

from .mock_backend import MockBackend


@dataclass
class HarnessConfig:
    model_path: str = "rwkv_mock"
    workspace_dir: str = "/tmp/mock"
    max_retries: int = 3
    strict_grammar: bool = True


@dataclass
class Context:
    session_id: str = "default"
    memory: List[str] = field(default_factory=list)
    tool_results: Dict[str, str] = field(default_factory=dict)


@dataclass
class State:
    checkpoint: str = ""
    attempts: int = 0

    def clone(self):
        return State(checkpoint=self.checkpoint, attempts=self.attempts)


class HarnessError(Exception):
    MockNotReady = "mock inference not connected"
    VerificationFailed = "verify failed"
    RollbackError = "rollback error"


class DomainHarness(ABC):
    """Abstract base, mirrors Rust trait."""

    @abstractmethod
    def name(self) -> str:
        ...

    def init(self, cfg: HarnessConfig) -> None:
        pass

    @abstractmethod
    def run(self, input_str: str, ctx: Context) -> str:
        ...

    @abstractmethod
    def verify(self, output: str) -> bool:
        ...

    @abstractmethod
    def rollback(self, state: State) -> State:
        ...

    def detailed_run(self, input_str: str) -> str:
        return f"[MOCK_DETAILED_{input_str}] input_length={len(input_str)} output_generated"


@dataclass
class LoopResult:
    output: str
    success: bool
    attempts: int
    rollback_count: int
    final_state: State
    history: List[State] = field(default_factory=list)


class ExecutionLoop:
    """Mirrors crates/harness/src/loop/mod.rs"""

    def __init__(self, max_attempts: int = 3):
        self.max_attempts = max_attempts

    def execute(self, agent: DomainHarness, input_str: str, ctx: Context) -> LoopResult:
        state = State()
        history: List[State] = []
        output = ""
        success = False

        for attempt in range(self.max_attempts):
            try:
                r = agent.run(input_str, ctx)
                output = r
                if agent.verify(output):
                    success = True
                    state.attempts = attempt + 1
                    state.checkpoint = f"check_{attempt}"
                    break
                else:
                    state = agent.rollback(state)
                    history.append(state.clone())
            except Exception:
                state = agent.rollback(state)
                history.append(state.clone())

        return LoopResult(
            output=output,
            success=success,
            attempts=state.attempts,
            rollback_count=len(history),
            final_state=state,
            history=history,
        )
