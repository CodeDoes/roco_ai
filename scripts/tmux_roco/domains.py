"""
Domains — Python mirror of all crates/harness/src/*.rs domain harnesses
11 domains + WritingAgent special + full_stack stack runner + 70 use cases aggregate.

Mirrors the Rust harness interface so tmux panes can switch domains instantly.
"""
from __future__ import annotations
from typing import Dict, List, Type
from dataclasses import dataclass
from .framework import DomainHarness, HarnessConfig, Context, State, HarnessError
from .mock_backend import MockBackend


class BaseAgent(DomainHarness):
    def __init__(self, domain_name: str):
        self._domain = domain_name
        self._backend = MockBackend(name=domain_name)
        self._config: HarnessConfig | None = None

    def name(self) -> str:
        return self._domain

    def init(self, cfg: HarnessConfig) -> None:
        self._config = cfg

    def run(self, input_str: str, ctx: Context) -> str:
        # Mirrors Rust: format!("{} ctx={:?}", input, ctx.session_id)
        prompt = f"{input_str} ctx={ctx.session_id!r}"
        # Domain-specific prefix adds realism
        prefixed = f"[{self._domain}] {prompt}"
        return self._backend.generate(prefixed)

    def verify(self, output: str) -> bool:
        return "MOCK_INFERENCE_RESULT" in output

    def rollback(self, state: State) -> State:
        return State(checkpoint=state.checkpoint, attempts=state.attempts + 1)

    def detailed_run(self, input_str: str) -> str:
        return self._backend.detailed_run(f"[{self._domain}] {input_str}")


# All 11 domains listed in harness lib.rs
def _make_domain(name: str):
    # dynamic class maker to keep name distinct but same logic
    return type(f"{name.capitalize()}Agent", (BaseAgent,), {"__init__": lambda self: BaseAgent.__init__(self, name)})

# Explicit classes for import clarity
class ChatAgent(BaseAgent):
    def __init__(self): super().__init__("chat")

class CodingAgent(BaseAgent):
    def __init__(self): super().__init__("coding")

class WritingAgent(BaseAgent):
    def __init__(self): super().__init__("writing")
    def run(self, input_str: str, ctx: Context) -> str:
        out = self._backend.generate(f"analyze story: {input_str} session={ctx.session_id!r}")
        if "MOCK" in out:
            return out
        raise HarnessError("mock inference not connected")

class BrowserAgent(BaseAgent):
    def __init__(self): super().__init__("browser")

class EmailAgent(BaseAgent):
    def __init__(self): super().__init__("email")

class ResearchAgent(BaseAgent):
    def __init__(self): super().__init__("research")

class OrganizationAgent(BaseAgent):
    def __init__(self): super().__init__("organization")

class HtmlAgent(BaseAgent):
    def __init__(self): super().__init__("html")

class PetAgent(BaseAgent):
    def __init__(self): super().__init__("pet")

class DebugAgent(BaseAgent):
    def __init__(self): super().__init__("debug")

class FullStackAgent(BaseAgent):
    def __init__(self): super().__init__("full_stack")

class AggregateAgent(BaseAgent):
    def __init__(self): super().__init__("aggregate")

# Registry
DOMAIN_REGISTRY: Dict[str, Type[BaseAgent]] = {
    "chat": ChatAgent,
    "coding": CodingAgent,
    "writing": WritingAgent,
    "browser": BrowserAgent,
    "email": EmailAgent,
    "research": ResearchAgent,
    "organization": OrganizationAgent,
    "html": HtmlAgent,
    "pet": PetAgent,
    "debug": DebugAgent,
    "full_stack": FullStackAgent,
    "aggregate": AggregateAgent,
}

# 70 use cases aggregate — mirrors use_cases/all_70.rs
USE_CASES_70 = {
    "privacy_security": 7,
    "offline_edge": 5,
    "cost_infrastructure": 4,
    "personalization_fine_tuning": 5,
    "creative_arts": 6,
    "coding": 5,
    "productivity_knowledge": 5,
    "home_automation": 4,
    "gaming": 4,
    "education": 4,
    "data_analysis": 4,
    "accessibility": 4,
    "research_tinkering": 5,
    "niche_edge": 8,
}

def list_domains() -> List[str]:
    return sorted(DOMAIN_REGISTRY.keys())

def list_use_cases() -> Dict[str, int]:
    return USE_CASES_70.copy()

def total_use_cases() -> int:
    return sum(USE_CASES_70.values())

def create_agent(domain: str) -> BaseAgent:
    """Factory — creates agent for domain, fallback to generic BaseAgent."""
    cls = DOMAIN_REGISTRY.get(domain)
    if cls:
        return cls()
    # generic for unknown — still functional
    return BaseAgent(domain)


@dataclass
class StackResult:
    """Mirrors full_stack.rs StackResult"""
    output: str
    success: bool
    attempts: int
    rollback_count: int
    final_state: State


class StackRunner:
    """Full stack runner — mirrors crates/harness/src/full_stack.rs"""

    @staticmethod
    def run_all(input_str: str) -> StackResult:
        from .framework import ExecutionLoop, Context, State
        cfg = HarnessConfig(
            model_path="rwkv_mock",
            workspace_dir="/tmp/mock",
            max_retries=3,
            strict_grammar=True,
        )
        agent = CodingAgent()
        agent.init(cfg)
        ctx = Context(
            session_id="full_stack_01",
            memory=[input_str],
            tool_results={},
        )
        state = State()
        history = []
        output = ""
        success = False

        loop_runner = ExecutionLoop(max_attempts=cfg.max_retries)
        res = loop_runner.execute(agent, input_str, ctx)
        return StackResult(
            output=res.output,
            success=res.success,
            attempts=res.attempts,
            rollback_count=res.rollback_count,
            final_state=res.final_state,
        )
