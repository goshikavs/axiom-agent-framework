"""OpenAI Agents SDK reference integration via documented tracing hooks.

Falls back to a local mock agent if `openai-agents` is not installed, still emitting
the same OTLP shape. Goshikq does not execute or approve the SDK's tools.
"""

from __future__ import annotations

import asyncio
import sys
from typing import Any

from goshikq_ai.client import RuntimeClient
from goshikq_ai.otel import OtlpTracer


async def _run_with_sdk(tracer: OtlpTracer) -> dict[str, Any]:
    try:
        from agents import Agent, Runner, function_tool
        from agents.tracing import TracingProcessor, add_trace_processor
    except ImportError:
        return {}

    class GoshikqProcessor(TracingProcessor):  # type: ignore[misc]
        def __init__(self) -> None:
            self._root = None

        def on_trace_start(self, trace: Any) -> None:
            self._root = tracer.start(
                getattr(trace, "name", "agent"),
                attributes={"gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": getattr(trace, "name", "openai-agents")},
            )

        def on_span_start(self, span: Any) -> None:
            return None

        def on_span_end(self, span: Any) -> None:
            kind = type(span).__name__.lower()
            attrs: dict[str, Any] = {"openai.agents.span": kind}
            name = getattr(span, "name", kind)
            parent = self._root
            s = tracer.start(name, parent=parent, attributes=attrs)
            err = getattr(span, "error", None)
            tracer.end(s, status_code=2 if err else 1, error=str(err) if err else None)

        def on_trace_end(self, trace: Any) -> None:
            if self._root:
                tracer.end(self._root)

        def shutdown(self) -> None:
            return None

        def force_flush(self) -> None:
            return None

    add_trace_processor(GoshikqProcessor())

    @function_tool
    def lookup_order(order_id: str) -> dict[str, str]:
        return {"order_id": order_id, "status": "paid"}

    agent = Agent(name="refund-agent", instructions="Look up the order. Do not claim a refund succeeded.", tools=[lookup_order])
    result = await Runner.run(agent, "lookup ORD-2048")
    return {"final_output": str(getattr(result, "final_output", result))}


async def _run_mock(tracer: OtlpTracer) -> dict[str, Any]:
    root = tracer.start("invoke_agent", attributes={"gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": "refund-agent"})
    tool = tracer.start(
        "lookup_order",
        parent=root,
        attributes={"gen_ai.operation.name": "execute_tool", "gen_ai.tool.name": "lookup_order"},
    )
    tracer.end(tool)
    llm = tracer.start(
        "chat",
        parent=root,
        attributes={"gen_ai.operation.name": "chat", "gen_ai.request.model": "mock"},
    )
    tracer.end(llm)
    tracer.end(root)
    return {"final_output": "order paid (mock path; openai-agents not installed)"}


async def main() -> None:
    client = RuntimeClient()
    tracer = OtlpTracer(service_name="openai-agents-refund", framework="openai-agents", client=client)
    sdk_result = await _run_with_sdk(tracer)
    if not sdk_result:
        print("openai-agents not installed; emitting mock OTLP with the same span conventions", file=sys.stderr)
        sdk_result = await _run_mock(tracer)
    ingest = await tracer.flush()
    print("agent result", sdk_result)
    print("otlp ingest", ingest)
    print("Goshikq did not control OpenAI Agents SDK actions.")
    await client.aclose()


if __name__ == "__main__":
    asyncio.run(main())
