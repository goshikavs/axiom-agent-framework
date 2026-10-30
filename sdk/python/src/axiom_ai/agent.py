"""Agent definition."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .errors import ConfigurationError
from .models.base import ModelAdapter
from .models.registry import resolve_model
from .tools import Tool


@dataclass
class RetryPolicy:
    max_attempts: int = 2
    initial_backoff_ms: int = 200
    backoff_multiplier: float = 2.0

    def to_dict(self) -> dict[str, Any]:
        return {"max_attempts": self.max_attempts, "initial_backoff_ms": self.initial_backoff_ms, "backoff_multiplier": self.backoff_multiplier}


@dataclass
class ExecutionLimits:
    max_steps: int = 10
    deadline_seconds: int = 300
    max_total_tokens: int | None = None
    max_cost_usd: float | None = None
    max_replans: int = 2
    max_tool_result_bytes: int = 64 * 1024
    external_wait_seconds: int = 30

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "max_steps": self.max_steps,
            "deadline_seconds": self.deadline_seconds,
            "max_replans": self.max_replans,
            "max_tool_result_bytes": self.max_tool_result_bytes,
            "external_wait_seconds": self.external_wait_seconds,
        }
        if self.max_total_tokens is not None:
            d["max_total_tokens"] = self.max_total_tokens
        if self.max_cost_usd is not None:
            d["max_cost_usd"] = self.max_cost_usd
        return d


@dataclass
class Agent:
    """A model-driven decision-making component.

    ``model`` is either a provider-qualified identifier (``"openai/<model>"``,
    ``"anthropic/<model>"``) resolved through the adapter registry, or a
    :class:`~axiom_ai.models.base.ModelAdapter` instance (e.g. ``MockModel``).
    """

    name: str
    model: str | ModelAdapter
    instructions: str = ""
    description: str = ""
    tools: list[Tool] = field(default_factory=list)
    execution_limits: ExecutionLimits = field(default_factory=ExecutionLimits)
    retry_policy: RetryPolicy = field(default_factory=RetryPolicy)
    context_policy: dict[str, Any] = field(default_factory=dict)
    permissions: list[str] = field(default_factory=list)
    metadata: dict[str, Any] = field(default_factory=dict)
    id: str | None = None

    def __post_init__(self) -> None:
        if not self.name.strip():
            raise ConfigurationError("Agent.name is required")
        names = [t.name for t in self.tools]
        if len(set(names)) != len(names):
            raise ConfigurationError(f"tool names must be unique: {names}")
        for t in self.tools:
            if not isinstance(t, Tool):
                raise ConfigurationError(f"tools must be created with @tool; got {type(t).__name__}")
        self._adapter: ModelAdapter = resolve_model(self.model)

    @property
    def adapter(self) -> ModelAdapter:
        return self._adapter

    @property
    def model_id(self) -> str:
        return self._adapter.model_id

    def tool(self, name: str) -> Tool:
        for t in self.tools:
            if t.name == name:
                return t
        raise KeyError(name)

    def to_definition(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "description": self.description,
            "model": self.model_id,
            "instructions": self.instructions,
            "tools": [t.to_spec() for t in self.tools],
            "execution_limits": self.execution_limits.to_dict(),
            "retry_policy": self.retry_policy.to_dict(),
            "context_policy": self.context_policy,
            "permissions": list(self.permissions),
            "metadata": {**self.metadata, **({"id": self.id} if self.id else {})},
        }
