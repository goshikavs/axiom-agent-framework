"""Goal contracts: success predicates and invariants evaluated against authoritative state."""

from __future__ import annotations

import inspect
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from enum import Enum
from typing import Any

from .errors import ConfigurationError
from .state import StateSnapshot

Predicate = Callable[[StateSnapshot], "bool | InvariantOutcome | Awaitable[bool | InvariantOutcome]"]


@dataclass
class InvariantOutcome:
    """Return from a predicate/invariant to attach a reason and evidence."""

    satisfied: bool
    reason: str | None = None
    expected: Any = None
    actual: Any = None


class Verification(str, Enum):
    REQUIRED = "required"
    OPTIONAL = "optional"
    NONE = "none"


@dataclass
class Goal:
    """What the agent must achieve, and the constraints that must always hold.

    ``success`` is evaluated by the runtime against a fresh :class:`StateSnapshot`; it is
    never satisfied by the model's own claims. ``invariants`` are evaluated before
    execution, after each step, and after approvals.
    """

    description: str
    success: Predicate | None = None
    invariants: list[Predicate] = field(default_factory=list)
    max_steps: int | None = None
    deadline_seconds: int | None = None
    external_wait_seconds: int | None = None
    max_total_tokens: int | None = None
    max_cost_usd: float | None = None
    verification: Verification = Verification.REQUIRED
    metadata: dict[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        if not self.description.strip():
            raise ConfigurationError("Goal.description is required")
        names = [_name(p) for p in self.invariants]
        if len(set(names)) != len(names):
            raise ConfigurationError(f"invariant names must be unique: {names}")

    @property
    def success_name(self) -> str | None:
        return _name(self.success) if self.success else None

    @property
    def invariant_names(self) -> list[str]:
        return [_name(p) for p in self.invariants]

    def to_definition(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "description": self.description,
            "success_predicate": self.success_name,
            "invariants": self.invariant_names,
            "verification_policy": self.verification.value,
            "metadata": self.metadata,
        }
        if self.max_steps is not None:
            d["max_steps"] = self.max_steps
        if self.deadline_seconds is not None:
            d["deadline_seconds"] = self.deadline_seconds
        if self.external_wait_seconds is not None:
            d["external_wait_seconds"] = self.external_wait_seconds
        if self.max_total_tokens is not None or self.max_cost_usd is not None:
            d["budget"] = {"max_total_tokens": self.max_total_tokens, "max_cost_usd": self.max_cost_usd}
        return d

    async def evaluate(self, snapshot: StateSnapshot, *, evaluate_success: bool) -> dict[str, Any]:
        """Produce a ``GoalEvaluateResult`` payload for the runtime."""
        invariants = []
        for p in self.invariants:
            out = await _run(p, snapshot)
            invariants.append({"name": _name(p), "satisfied": out.satisfied, "reason": out.reason})
        result: dict[str, Any] = {"invariants": invariants, "evidence": {"source": snapshot.source, "version": snapshot.version}}
        if evaluate_success and self.success is not None:
            out = await _run(self.success, snapshot)
            result["success"] = out.satisfied
            result["reason"] = out.reason
            result["evidence"].update({"predicate": self.success_name, "expected": out.expected, "actual": out.actual})
        return result


def _name(p: Callable[..., Any]) -> str:
    return getattr(p, "__name__", p.__class__.__name__)


async def _run(p: Predicate, snapshot: StateSnapshot) -> InvariantOutcome:
    try:
        res = p(snapshot)
        if inspect.isawaitable(res):
            res = await res
    except Exception as e:  # a crashing predicate is a failed check, never a pass
        return InvariantOutcome(False, reason=f"{_name(p)} raised {type(e).__name__}: {e}")
    if isinstance(res, InvariantOutcome):
        return res
    if isinstance(res, bool):
        return InvariantOutcome(res, reason=None if res else f"{_name(p)} returned False")
    return InvariantOutcome(False, reason=f"{_name(p)} returned non-boolean {type(res).__name__}")
