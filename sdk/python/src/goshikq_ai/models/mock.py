"""Deterministic offline model for tests, examples, and fault injection."""

from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass, field
from typing import Any

from ..errors import ModelAdapterError
from .base import Capabilities, ModelResponse, StreamEvent, TokenUsage, ToolCall


@dataclass
class ModelTurn:
    """One scripted assistant turn: either text or tool calls (or both)."""

    content: str | None = None
    tool_calls: list[tuple[str, dict[str, Any]]] = field(default_factory=list)
    finish_reason: str | None = None
    usage: TokenUsage | None = None
    # Fault injection
    fail: ModelAdapterError | None = None
    delay_seconds: float = 0.0


Policy = Callable[[list[dict[str, Any]], list[dict[str, Any]], int], ModelTurn]


class MockModel:
    """Scripted or policy-driven model with synthetic token accounting.

    * ``script``: a list of :class:`ModelTurn` consumed in order (the last turn repeats).
    * ``policy``: ``fn(messages, tools, turn_index) -> ModelTurn`` for reactive scripts that
      inspect tool results, e.g. the retail refund example.
    """

    provider = "mock"
    capabilities = Capabilities(tool_calling=True, structured_output=True, response_usage=True)

    def __init__(self, script: list[ModelTurn] | None = None, policy: Policy | None = None, *, model_id: str = "mock/deterministic") -> None:
        if (script is None) == (policy is None):
            raise ValueError("provide exactly one of script= or policy=")
        if not model_id.startswith("mock/"):
            raise ValueError("MockModel ids must start with `mock/`")
        self.model_id = model_id
        self._script = list(script or [])
        self._policy = policy
        self._turn = 0
        self._call_counter = 0

    def _next_turn(self, messages: list[dict[str, Any]], tools: list[dict[str, Any]]) -> ModelTurn:
        idx = self._turn
        self._turn += 1
        if self._policy is not None:
            return self._policy(messages, tools, idx)
        if not self._script:
            return ModelTurn(content="(empty script)")
        return self._script[min(idx, len(self._script) - 1)]

    async def generate(self, messages: list[dict[str, Any]], tools: list[dict[str, Any]], config: dict[str, Any]) -> ModelResponse:
        turn = self._next_turn(messages, tools)
        if turn.delay_seconds:
            await asyncio.sleep(turn.delay_seconds)
        if turn.fail is not None:
            raise turn.fail
        calls = []
        for name, args in turn.tool_calls:
            self._call_counter += 1
            calls.append(ToolCall(id=f"call_{self._call_counter}", name=name, arguments=args))
        usage = turn.usage or _synthetic_usage(messages, turn)
        return ModelResponse(
            content=turn.content,
            tool_calls=calls,
            usage=usage,
            finish_reason=turn.finish_reason or ("tool_calls" if calls else "stop"),
            provider_response_id=f"mock-{self._turn}",
            latency_ms=int(turn.delay_seconds * 1000),
            model=self.model_id,
            provider=self.provider,
        )

    async def stream(self, messages: list[dict[str, Any]], tools: list[dict[str, Any]], config: dict[str, Any]) -> AsyncIterator[StreamEvent]:
        resp = await self.generate(messages, tools, config)
        if resp.content:
            yield StreamEvent(kind="text_delta", text=resp.content)
        for c in resp.tool_calls:
            yield StreamEvent(kind="tool_call", tool_call=c)
        yield StreamEvent(kind="usage", usage=resp.usage)
        yield StreamEvent(kind="done", finish_reason=resp.finish_reason)


def _synthetic_usage(messages: list[dict[str, Any]], turn: ModelTurn) -> TokenUsage:
    """Deterministic, clearly synthetic: ~4 chars per token."""
    prompt_chars = sum(len(json.dumps(m, sort_keys=True)) for m in messages)
    out_chars = len(turn.content or "") + sum(len(json.dumps(a, sort_keys=True)) + len(n) for n, a in turn.tool_calls)
    return TokenUsage(input_tokens=max(1, prompt_chars // 4), output_tokens=max(1, out_chars // 4), cached_tokens=0)


def last_tool_result(messages: list[dict[str, Any]], tool_name: str | None = None) -> dict[str, Any] | None:
    """Helper for policies: the most recent ``tool`` message (optionally by name), parsed."""
    for m in reversed(messages):
        if m.get("role") == "tool" and (tool_name is None or m.get("name") == tool_name):
            try:
                parsed = json.loads(m.get("content") or "{}")
                return parsed if isinstance(parsed, dict) else {"_value": parsed}
            except json.JSONDecodeError:
                return {"_raw": m.get("content")}
    return None


def tool_results(messages: list[dict[str, Any]], tool_name: str) -> list[dict[str, Any]]:
    out = []
    for m in messages:
        if m.get("role") == "tool" and m.get("name") == tool_name:
            try:
                parsed = json.loads(m.get("content") or "{}")
                out.append(parsed if isinstance(parsed, dict) else {"_value": parsed})
            except json.JSONDecodeError:
                out.append({"_raw": m.get("content")})
    return out
