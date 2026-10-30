"""OpenAI adapter (Chat Completions API with native tool calling).

Status: experimental. Requires ``pip install axiom-ai[openai]`` and ``OPENAI_API_KEY``.
"""

from __future__ import annotations

import json
import os
import time
from collections.abc import AsyncIterator
from typing import Any

from ..errors import ConfigurationError, ModelAdapterError
from .base import Capabilities, ModelResponse, StreamEvent, TokenUsage, ToolCall, parse_tool_arguments


def _to_openai_messages(messages: list[dict[str, Any]]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for m in messages:
        role = m.get("role")
        if role == "assistant":
            entry: dict[str, Any] = {"role": "assistant", "content": m.get("content")}
            if m.get("tool_calls"):
                entry["tool_calls"] = [
                    {"id": c["id"], "type": "function", "function": {"name": c["name"], "arguments": json.dumps(c.get("arguments") or {})}}
                    for c in m["tool_calls"]
                ]
            out.append(entry)
        elif role == "tool":
            out.append({"role": "tool", "tool_call_id": m.get("tool_call_id"), "content": m.get("content") or ""})
        else:
            out.append({"role": role, "content": m.get("content") or ""})
    return out


class OpenAIAdapter:
    provider = "openai"
    capabilities = Capabilities(tool_calling=True, response_usage=True, prompt_caching=True)

    def __init__(self, model: str, *, api_key: str | None = None, base_url: str | None = None, timeout: float = 60.0) -> None:
        try:
            from openai import AsyncOpenAI
        except ImportError as e:  # pragma: no cover - import guard
            raise ConfigurationError("install the OpenAI extra: pip install 'axiom-ai[openai]'") from e
        key = api_key or os.environ.get("OPENAI_API_KEY")
        if not key:
            raise ConfigurationError("OPENAI_API_KEY is not set; the quickstart uses mock/deterministic and needs no key")
        self.model = model
        self.model_id = f"openai/{model}"
        self._client = AsyncOpenAI(api_key=key, base_url=base_url, timeout=timeout)

    async def generate(self, messages: list[dict[str, Any]], tools: list[dict[str, Any]], config: dict[str, Any]) -> ModelResponse:
        import openai

        kwargs: dict[str, Any] = {"model": self.model, "messages": _to_openai_messages(messages)}
        if tools:
            kwargs["tools"] = [{"type": "function", "function": {"name": t["name"], "description": t.get("description") or "", "parameters": t["parameters"]}} for t in tools]
            kwargs["tool_choice"] = "auto"
        for k in ("temperature", "max_tokens", "max_completion_tokens", "top_p"):
            if k in config:
                kwargs[k] = config[k]
        t0 = time.perf_counter()
        try:
            resp = await self._client.chat.completions.create(**kwargs)
        except (openai.RateLimitError, openai.APIConnectionError, openai.APITimeoutError, openai.InternalServerError) as e:
            raise ModelAdapterError(str(e), code=type(e).__name__, retryable=True) from e
        except openai.APIStatusError as e:
            raise ModelAdapterError(str(e), code=type(e).__name__, retryable=False, details={"status": e.status_code}) from e
        latency = int((time.perf_counter() - t0) * 1000)
        choice = resp.choices[0]
        calls = [ToolCall(id=c.id, name=c.function.name, arguments=parse_tool_arguments(c.function.arguments)) for c in (choice.message.tool_calls or [])]
        usage = None
        if resp.usage is not None:
            cached = getattr(getattr(resp.usage, "prompt_tokens_details", None), "cached_tokens", None)
            usage = TokenUsage(input_tokens=resp.usage.prompt_tokens, output_tokens=resp.usage.completion_tokens, cached_tokens=cached)
        return ModelResponse(
            content=choice.message.content,
            tool_calls=calls,
            usage=usage,
            finish_reason=choice.finish_reason,
            provider_response_id=resp.id,
            latency_ms=latency,
            model=f"openai/{resp.model}" if resp.model else self.model_id,
            provider=self.provider,
        )

    async def stream(self, messages: list[dict[str, Any]], tools: list[dict[str, Any]], config: dict[str, Any]) -> AsyncIterator[StreamEvent]:
        # Streaming is not wired into the runtime protocol yet; fall back to generate().
        resp = await self.generate(messages, tools, config)
        if resp.content:
            yield StreamEvent(kind="text_delta", text=resp.content)
        for c in resp.tool_calls:
            yield StreamEvent(kind="tool_call", tool_call=c)
        yield StreamEvent(kind="usage", usage=resp.usage)
        yield StreamEvent(kind="done", finish_reason=resp.finish_reason)
