"""LangGraph controls this refund graph. Goshikq only receives OTLP spans.

Goshikq does not intercept node writes and does not treat the trace as a verified payout.

Usage:
    pip install langgraph
    python main.py                 # requires goshikq-server on 127.0.0.1:7400
"""

from __future__ import annotations

import asyncio
import sys
from typing import Any, TypedDict

from goshikq_ai.client import RuntimeClient
from goshikq_ai.otel import OtlpTracer, Span

try:
    from langgraph.graph import END, StateGraph
except ImportError:
    print("pip install langgraph", file=sys.stderr)
    raise SystemExit(2)


class RefundState(TypedDict):
    order_id: str
    eligible: bool
    refunded: bool
    error: str | None


STORE = {"ORD-2048": {"status": "paid", "eligible": True}}


def build_graph(tracer: OtlpTracer, root: Span):
    def lookup(state: RefundState) -> RefundState:
        span = tracer.start("lookup_order", parent=root, attributes={"langgraph.node": "lookup", "openinference.span.kind": "CHAIN"})
        order = STORE.get(state["order_id"])
        out = {**state, "eligible": bool(order and order.get("eligible"))}
        tracer.end(span)
        return out

    def decide(state: RefundState) -> RefundState:
        span = tracer.start(
            "chat",
            parent=root,
            attributes={"gen_ai.operation.name": "chat", "gen_ai.request.model": "mock-graph", "openinference.span.kind": "LLM"},
        )
        tracer.end(span, attributes={"gen_ai.usage.input_tokens": 8, "gen_ai.usage.output_tokens": 2})
        return state

    def refund(state: RefundState) -> RefundState:
        span = tracer.start(
            "submit_refund",
            parent=root,
            attributes={"gen_ai.operation.name": "execute_tool", "gen_ai.tool.name": "submit_refund", "langgraph.node": "refund"},
        )
        if not state["eligible"]:
            tracer.end(span, status_code=2, error="ineligible")
            return {**state, "error": "ineligible"}
        STORE[state["order_id"]]["status"] = "refunded"
        tracer.end(span, attributes={"goshikq.state_snapshot": {"orders": {state["order_id"]: {"status": "refunded"}}}})
        return {**state, "refunded": True}

    g = StateGraph(RefundState)
    g.add_node("lookup", lookup)
    g.add_node("decide", decide)
    g.add_node("refund", refund)
    g.set_entry_point("lookup")
    g.add_edge("lookup", "decide")
    g.add_edge("decide", "refund")
    g.add_edge("refund", END)
    return g.compile()


async def main() -> None:
    client = RuntimeClient()
    tracer = OtlpTracer(service_name="langgraph-refund", framework="langgraph", client=client)
    root = tracer.start("invoke_agent", attributes={"gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": "langgraph-refund"})
    app = build_graph(tracer, root)
    result: dict[str, Any] = app.invoke({"order_id": "ORD-2048", "eligible": False, "refunded": False, "error": None})
    tracer.end(root)
    ingest = await tracer.flush()
    print("langgraph result", result)
    print("otlp ingest", ingest)
    print("Goshikq did not control this graph. Studio will show source_framework=langgraph.")
    print("The run is UNVERIFIED unless you attach an Goshikq goal/state provider separately.")
    await client.aclose()


if __name__ == "__main__":
    asyncio.run(main())
