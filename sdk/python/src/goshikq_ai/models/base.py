"""Provider-neutral model interface and normalized structures."""

from __future__ import annotations

import json
from collections.abc import AsyncIterator
from dataclasses import dataclass, field
from typing import Any, Protocol, runtime_checkable


@dataclass(frozen=True)
class Capabilities:
    streaming: bool = False
    tool_calling: bool = False
    structured_output: bool = False
    multimodal_input: bool = False
    response_usage: bool = False
    prompt_caching: bool = False
    reasoning_controls: bool = False

    def to_dict(self) -> dict[str, bool]:
        return self.__dict__.copy()


@dataclass
class ToolCall:
    id: str
    name: str
    arguments: dict[str, Any]

    def to_dict(self) -> dict[str, Any]:
        return {"id": self.id, "name": self.name, "arguments": self.arguments}


@dataclass
class TokenUsage:
    input_tokens: int = 0
    output_tokens: int = 0
    cached_tokens: int | None = None

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {"input_tokens": self.input_tokens, "output_tokens": self.output_tokens}
        if self.cached_tokens is not None:
            d["cached_tokens"] = self.cached_tokens
        return d


@dataclass
class ModelResponse:
    """Normalized completion."""

    content: str | None = None
    tool_calls: list[ToolCall] = field(default_factory=list)
    usage: TokenUsage | None = None
    finish_reason: str | None = None
    provider_response_id: str | None = None
    latency_ms: int | None = None
    model: str | None = None
    provider: str | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "content": self.content,
            "tool_calls": [t.to_dict() for t in self.tool_calls],
            "usage": self.usage.to_dict() if self.usage else None,
            "finish_reason": self.finish_reason,
            "provider_response_id": self.provider_response_id,
            "latency_ms": self.latency_ms,
            "model": self.model,
            "provider": self.provider,
        }


@dataclass
class StreamEvent:
    """Incremental output. ``kind`` is ``text_delta`` | ``tool_call`` | ``usage`` | ``done``."""

    kind: str
    text: str | None = None
    tool_call: ToolCall | None = None
    usage: TokenUsage | None = None
    finish_reason: str | None = None


@runtime_checkable
class ModelAdapter(Protocol):
    """Implemented per provider. Messages and tools use the protocol wire format
    (``ModelMessage`` / ``ModelToolSchema`` dictionaries)."""

    model_id: str
    provider: str
    capabilities: Capabilities

    async def generate(self, messages: list[dict[str, Any]], tools: list[dict[str, Any]], config: dict[str, Any]) -> ModelResponse: ...

    def stream(self, messages: list[dict[str, Any]], tools: list[dict[str, Any]], config: dict[str, Any]) -> AsyncIterator[StreamEvent]: ...


def parse_tool_arguments(raw: Any) -> dict[str, Any]:
    """Providers return JSON strings; treat anything unparsable as an empty object so the
    runtime's schema validation rejects it with a clear error."""
    if isinstance(raw, dict):
        return raw
    if isinstance(raw, str):
        try:
            parsed = json.loads(raw or "{}")
            return parsed if isinstance(parsed, dict) else {"_value": parsed}
        except json.JSONDecodeError:
            return {"_unparsable": raw[:200]}
    return {}
