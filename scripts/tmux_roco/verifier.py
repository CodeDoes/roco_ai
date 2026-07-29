"""
Verifier — Python mirror of crates/harness/src/verifier.rs
Deterministic verifiers for output validation.
"""
from __future__ import annotations
from typing import Set, List


class Verifier:
    def __init__(self, min_length: int = 10, required_patterns: List[str] | None = None, forbidden_words: Set[str] | None = None):
        self.min_length = min_length
        self.required_patterns = required_patterns or ["MOCK_INFERENCE_RESULT"]
        self.forbidden_words = set(forbidden_words or [])

    def verify(self, output: str) -> bool:
        if len(output) < self.min_length:
            return False
        for w in self.forbidden_words:
            if w in output:
                return False
        for pat in self.required_patterns:
            if pat not in output:
                return False
        return True

    def score(self, output: str) -> float:
        score = 1.0
        if len(output) < self.min_length:
            score *= 0.5
        for pat in self.required_patterns:
            if pat in output:
                score *= 1.2
            else:
                score *= 0.3
        return min(score, 1.0)

    def explain(self, output: str) -> str:
        if self.verify(output):
            return f"PASS: verified (len={len(output)}, required_patterns_matches)"
        else:
            return f"FAIL: min_length={self.min_length}, required={len(self.required_patterns)}, forbidden_check"
