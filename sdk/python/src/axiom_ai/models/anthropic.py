"""Anthropic adapter (Messages API with native tool use).

Status: experimental. Requires ``pip install axiom-ai[anthropic]`` and ``ANTHROPIC_API_KEY``.
"""

from __future__ import annotations

import os
import time
from collections.abc import AsyncIterator
from typing import Any

from ..errors import ConfigurationError, ModelAdapterError
from .base import Capabilities, ModelResponse, StreamEvent, TokenUsage, ToolCall


def _to_anthropic(messages: list[dict[str, Any]]) -> tuple[str | None, list[dict[str, Any]]]:
    system_parts: list[str] = []
    out: list[dict[str, Any]] = []
    for m in messages:
        role = m.get("role")
        if role == "system":
            if m.get("content"):
                system_parts.append(m["content"])
        elif role == "assistant":
            blocks: list[dict[str, Any]] = []
            if m.get("content"):
                blocks.append({"type": "text", "text": m["content"]})
            for c in m.get("tool_calls") or []:
                blocks.append({"type": "tool_use", "id": c["id"], "name": c["name"], "input": c.get("arguments") or {}})
            out.append({"role": "assistant", "content": blocks or [{"type": "text", "text": ""}]})
        elif role == "tool":
            block = {"type": "tool_result", "tool_use_id": m.get("tool_call_id"), "content": m.get("content") or ""}
            # Consecutive tool results must share one user message.
            if out and out[-1]["role"] == "user" and isinstance(out[-1]["content"], list) and out[-1]["content"] and out[-1]["content"][0].get("type") == "tool_result":
                out[-1]["content"].append(block)
            else:
                out.append({"role": "user", "content": [block]})
        else:
            out.append({"role": "user", "content": m.get("content") or ""})
    return ("\n\n".join(system_parts) or None), out


class AnthropicAdapter:
    provider = "anthropic"
    capabilities = Capabilities(tool_calling=True, response_usage=True, prompt_caching=True)

    def __init__(self, model: str, *, api_key: str | None = None, max_tokens: int = 1024, timeout: float = 60.0) -> None:
        try:
            from anthropic import AsyncAnthropic
        except ImportError as e:  # pragma: no cover - import guard
            raise ConfigurationError("install the Anthropic extra: pip install 'axiom-ai[anthropic]'") from e
        key = api_key or os.environ.get("ANTHROPIC_API_KEY")
        if not key:
            raise ConfigurationError("ANTHROPIC_API_KEY is not set; the quickstart uses mock/deterministic and needs no key")
        self.model = model
        self.model_id = f"anthropic/{model}"
        self.max_tokens = max_tokens
        self._client = AsyncAnthropic(api_key=key, timeout=timeout)

    async def generate(self, messages: list[dict[str, Any]], tools: list[dict[str, Any]], config: dict[str, Any]) -> ModelResponse:
        import anthropic

        system, msgs = _to_anthropic(messages)
        kwargs: dict[str, Any] = {"model": self.model, "max_tokens": int(config.get("max_tokens", self.max_tokens)), "messages": msgs}
        if system:
            kwargs["system"] = system
        if tools:
            kwargs["tools"] = [{"name": t["name"], "description": t.get("description") or "", "input_schema": t["parameters"]} for t in tools]
        if "temperature" in config:
            kwargs["temperature"] = config["temperature"]
        t0 = time.perf_counter()
        try:
            resp = await self._client.messages.create(**kwargs)
        except (anthropic.RateLimitError, anthropic.APIConnectionError, anthropic.APITimeoutError, anthropic.InternalServerError) as e:
            raise ModelAdapterError(str(e), code=type(e).__name__, retryable=True) from e
        except anthropic.APIStatusError as e:
            raise ModelAdapterError(str(e), code=type(e).__name__, retryable=False, details={"status": e.status_code}) from e
        latency = int((time.perf_counter() - t0) * 1000)
        text_parts: list[str] = []
        calls: list[ToolCall] = []
        for block in resp.content:
            if block.type == "text":
                text_parts.append(block.text)
            elif block.type == "tool_use":
                calls.append(ToolCall(id=block.id, name=block.name, arguments=dict(block.input) if isinstance(block.input, dict) else {}))
        usage = None
        if resp.usage is not None:
            usage = TokenUsage(input_tokens=resp.usage.input_tokens, output_tokens=resp.usage.output_tokens, cached_tokens=getattr(resp.usage, "cache_read_input_tokens", None))
        return ModelResponse(
            content="\n".join(text_parts) or None,
            tool_calls=calls,
            usage=usage,
            finish_reason=resp.stop_reason,
            provider_response_id=resp.id,
            latency_ms=latency,
            model=f"anthropic/{resp.model}" if resp.model else self.model_id,
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
