from axiom_ai.otel import OtlpTracer, spans_to_otlp_json


def test_otlp_json_includes_parent_and_omits_unknown_tokens() -> None:
    tracer = OtlpTracer(service_name="langgraph-refund", framework="langgraph")
    root = tracer.start("invoke_agent", attributes={"gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": "refund"})
    child = tracer.start(
        "chat",
        parent=root,
        attributes={"gen_ai.operation.name": "chat", "gen_ai.request.model": "mock"},
    )
    tracer.end(child)
    tracer.end(root)
    body = tracer.payload()
    spans = body["resourceSpans"][0]["scopeSpans"][0]["spans"]
    assert spans[0]["parentSpanId"] == root.span_id
    keys = {a["key"] for a in spans[0]["attributes"]}
    assert "gen_ai.usage.input_tokens" not in keys
    assert body["resourceSpans"][0]["resource"]["attributes"][1]["value"]["stringValue"] == "langgraph"


def test_spans_to_otlp_json_int_attributes() -> None:
    from axiom_ai.otel import Span

    s = Span(
        trace_id="aa" * 16,
        span_id="bb" * 8,
        name="tool",
        attributes={"gen_ai.tool.name": "refund", "gen_ai.usage.input_tokens": 3},
    )
    body = spans_to_otlp_json([s], service_name="demo", framework="openai-agents")
    attrs = {a["key"]: a["value"] for a in body["resourceSpans"][0]["scopeSpans"][0]["spans"][0]["attributes"]}
    assert attrs["gen_ai.usage.input_tokens"] == {"intValue": "3"}
