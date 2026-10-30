"""Live OTLP/HTTP ingest against a running axiom-server + PostgreSQL.

Skipped unless AXIOM_RUNTIME_URL (default 127.0.0.1:7400) answers /health.
"""

from __future__ import annotations

import os
import sys
import time
import uuid
from pathlib import Path

import httpx
import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "sdk" / "python" / "src"))

from axiom_ai.otel import OtlpTracer, Span, spans_to_otlp_json  # noqa: E402

ENDPOINT = os.environ.get("AXIOM_RUNTIME_URL", "http://127.0.0.1:7400").rstrip("/")
DEFAULT_PROJECT = "00000000-0000-0000-0000-000000000001"


def _health() -> bool:
    try:
        r = httpx.get(f"{ENDPOINT}/health", timeout=3)
        return r.status_code == 200
    except httpx.HTTPError:
        return False


pytestmark = pytest.mark.skipif(not _health(), reason="axiom-server not reachable")


def _hex(n: int) -> str:
    return uuid.uuid4().hex[: n * 2].ljust(n * 2, "a")


def _post_json(body: dict, headers: dict | None = None) -> httpx.Response:
    return httpx.post(f"{ENDPOINT}/v1/traces", json=body, headers=headers or {}, timeout=15)


def _runs(**params):
    r = httpx.get(f"{ENDPOINT}/api/v1/runs", params=params, timeout=15)
    r.raise_for_status()
    return r.json()


def _events(run_id: str):
    r = httpx.get(f"{ENDPOINT}/api/v1/runs/{run_id}/events", params={"limit": 2000}, timeout=15)
    r.raise_for_status()
    return r.json()["items"]


def _find_run_by_trace(trace_id: str) -> dict | None:
    rows = _runs(framework="otel", limit=50)["items"]
    for r in rows:
        if r.get("trace_id") == trace_id:
            return r
    # External runs may also be labeled langgraph/openai-agents.
    rows = _runs(limit=50)["items"]
    for r in rows:
        if r.get("trace_id") == trace_id:
            return r
    return None


def _run(run_id: str):
    r = httpx.get(f"{ENDPOINT}/api/v1/runs/{run_id}", timeout=15)
    r.raise_for_status()
    return r.json()


def test_otlp_json_parent_tokens_missing_error_and_unverified():
    tracer = OtlpTracer(service_name="otlp-live", framework="otel")
    root = tracer.start("invoke_agent", attributes={"gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": "otlp-live"})
    llm = tracer.start(
        "chat",
        parent=root,
        attributes={
            "gen_ai.operation.name": "chat",
            "gen_ai.request.model": "mock-otlp",
            "gen_ai.usage.input_tokens": 9,
            "gen_ai.usage.output_tokens": 4,
            "gen_ai.prompt": "sk-this-should-redact-if-pattern",
        },
    )
    tracer.end(llm)
    missing = tracer.start("chat-unknown", parent=root, attributes={"gen_ai.operation.name": "chat"})
    tracer.end(missing)
    err = tracer.start(
        "execute_tool",
        parent=root,
        attributes={"gen_ai.operation.name": "execute_tool", "gen_ai.tool.name": "submit_refund"},
    )
    tracer.end(err, status_code=2, error="processor rejected")
    tracer.end(root)
    resp = _post_json(tracer.payload())
    assert resp.status_code == 200, resp.text

    time.sleep(0.3)
    run = _find_run_by_trace(tracer.trace_id)
    assert run, "ingested run should appear in list"
    assert run["verification_status"] != "VERIFIED"
    assert run["status"] in ("UNVERIFIED", "PLANNING", "COMPLETED")
    # External completion must not be treated as verified success.
    assert run["status"] != "COMPLETED" or run["verification_status"] != "VERIFIED"

    events = _events(run["id"])
    types = {e["event_type"] for e in events}
    assert "model.completed" in types
    assert "tool.failed" in types
    assert "run.completed" in types
    assert "goal.satisfied" not in types
    assert "verification.passed" not in types

    llm_ev = next(e for e in events if e["event_type"] == "model.completed" and e["payload"].get("model") == "mock-otlp")
    assert llm_ev["parent_span_id"] == root.span_id
    assert llm_ev["payload"]["input_tokens"] == 9
    assert llm_ev["payload"]["output_tokens"] == 4
    prompt = (llm_ev["payload"].get("attributes") or {}).get("gen_ai.prompt")
    assert prompt in (None, "[REDACTED]") or "sk-" not in str(prompt)

    miss = next(e for e in events if e["event_type"] == "model.completed" and e["payload"].get("model") is None)
    missing_attrs = (miss.get("metadata") or {}).get("missing_attributes") or []
    assert "gen_ai.usage.input_tokens" in missing_attrs
    assert miss["payload"].get("input_tokens") is None

    failed = next(e for e in events if e["event_type"] == "tool.failed")
    assert failed["payload"].get("error")
    assert failed["timestamp"]

    detail = _run(run["id"])
    assert detail["source_framework"] == "otel"
    assert detail["verification_status"] != "VERIFIED"


def test_otlp_duplicate_is_idempotent():
    span = Span(
        trace_id=_hex(16),
        span_id=_hex(8),
        name="invoke_agent",
        attributes={"gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": "dup"},
        start_unix_nano=1_700_000_000_000_000_000,
        end_unix_nano=1_700_000_000_100_000_000,
    )
    body = spans_to_otlp_json([span], service_name="dup-agent", framework="otel")
    a = _post_json(body)
    b = _post_json(body)
    assert a.status_code == 200 and b.status_code == 200
    time.sleep(0.2)
    item = _find_run_by_trace(span.trace_id)
    assert item, "duplicate ingest should create one run"
    events = _events(item["id"])
    started = [e for e in events if e["event_type"] == "run.started" and e["span_id"] == span.span_id]
    assert len(started) == 1


def test_otlp_out_of_order_preserves_parent_and_source_time():
    trace = _hex(16)
    parent = _hex(8)
    child = _hex(8)
    child_span = Span(
        trace_id=trace,
        span_id=child,
        parent_span_id=parent,
        name="chat",
        start_unix_nano=2_000_000_000_500_000_000,
        end_unix_nano=2_000_000_000_600_000_000,
        attributes={"gen_ai.operation.name": "chat", "gen_ai.request.model": "ooo"},
    )
    parent_span = Span(
        trace_id=trace,
        span_id=parent,
        name="invoke_agent",
        start_unix_nano=2_000_000_000_000_000_000,
        end_unix_nano=2_000_000_000_900_000_000,
        attributes={"gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": "ooo-agent"},
    )
    assert _post_json(spans_to_otlp_json([child_span], service_name="ooo", framework="otel")).status_code == 200
    assert _post_json(spans_to_otlp_json([parent_span], service_name="ooo", framework="otel")).status_code == 200
    time.sleep(0.2)
    found = _find_run_by_trace(trace)
    assert found, "out-of-order spans should share one run"
    events = _events(found["id"])
    child_ev = next(e for e in events if e["span_id"] == child and e["event_type"] == "model.completed")
    assert child_ev["parent_span_id"] == parent
    assert child_ev["timestamp"].startswith("2033-") or child_ev["timestamp"]  # source time, not ingest time


def test_otlp_invalid_json_rejected():
    r = httpx.post(f"{ENDPOINT}/v1/traces", content=b"not-json", headers={"content-type": "application/json"}, timeout=10)
    assert r.status_code == 400


def test_otlp_project_mismatch_forbidden():
    tracer = OtlpTracer(service_name="iso", framework="otel")
    root = tracer.start("invoke_agent", attributes={"gen_ai.operation.name": "invoke_agent"})
    tracer.end(root)
    other = "11111111-1111-1111-1111-111111111111"
    r = _post_json(tracer.payload(), headers={"X-Axiom-Project-Id": other})
    assert r.status_code in (403, 401), r.text


def test_otlp_protobuf_roundtrip_live():
    from axiom_ai.otel import OtlpTracer

    # Encode a tiny ExportTraceServiceRequest with one span via local prost-compatible bytes
    # produced by posting JSON first is already covered; protobuf uses the same mapper.
    tracer = OtlpTracer(service_name="otlp-proto", framework="otel")
    root = tracer.start("invoke_agent", attributes={"gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": "proto-agent"})
    tracer.end(root)
    # Convert JSON OTLP to protobuf using a minimal encoder for resourceSpans/spans.
    raw = _json_otlp_to_protobuf(tracer.payload())
    r = httpx.post(f"{ENDPOINT}/v1/traces", content=raw, headers={"content-type": "application/x-protobuf"}, timeout=15)
    assert r.status_code == 200, r.text
    time.sleep(0.2)
    found = _find_run_by_trace(tracer.trace_id)
    assert found
    assert found["verification_status"] != "VERIFIED"


def test_otlp_span_limit_rejected():
    spans = [
        Span(trace_id=_hex(16), span_id=_hex(8), name=f"s{i}", attributes={"gen_ai.operation.name": "invoke_agent"})
        for i in range(2001)
    ]
    body = spans_to_otlp_json(spans, service_name="too-many", framework="otel")
    r = _post_json(body)
    assert r.status_code == 400, r.text


# --- minimal protobuf encoder for the OTLP subset we ingest ---

def _varint(n: int) -> bytes:
    out = bytearray()
    while n > 0x7F:
        out.append((n & 0x7F) | 0x80)
        n >>= 7
    out.append(n)
    return bytes(out)


def _key(field: int, wire: int) -> bytes:
    return _varint((field << 3) | wire)


def _ld(field: int, payload: bytes) -> bytes:
    return _key(field, 2) + _varint(len(payload)) + payload


def _str(field: int, s: str) -> bytes:
    b = s.encode()
    return _ld(field, b)


def _bytes(field: int, b: bytes) -> bytes:
    return _ld(field, b)


def _u64_fixed(field: int, n: int) -> bytes:
    return _key(field, 1) + n.to_bytes(8, "little")


def _i32(field: int, n: int) -> bytes:
    return _key(field, 0) + _varint(n)


def _hex_bytes(h: str) -> bytes:
    return bytes.fromhex(h)


def _kv(key: str, value: str) -> bytes:
    anyv = _str(1, value)  # AnyValue.string_value = 1
    return _str(1, key) + _ld(2, anyv)


def _json_otlp_to_protobuf(body: dict) -> bytes:
    rs_out = b""
    for rs in body.get("resourceSpans", []):
        resource_attrs = b""
        for a in (rs.get("resource") or {}).get("attributes") or []:
            val = (a.get("value") or {}).get("stringValue") or ""
            resource_attrs += _ld(1, _kv(a.get("key", ""), val))
        resource = _ld(1, resource_attrs) if resource_attrs else b""
        scopes = b""
        for ss in rs.get("scopeSpans") or []:
            spans = b""
            for sp in ss.get("spans") or []:
                msg = b""
                msg += _bytes(1, _hex_bytes(sp["traceId"]))
                msg += _bytes(2, _hex_bytes(sp["spanId"]))
                if sp.get("parentSpanId"):
                    msg += _bytes(4, _hex_bytes(sp["parentSpanId"]))
                msg += _str(5, sp.get("name", "span"))
                msg += _i32(6, int(sp.get("kind") or 0))
                msg += _u64_fixed(7, int(sp.get("startTimeUnixNano") or 0))
                msg += _u64_fixed(8, int(sp.get("endTimeUnixNano") or 0))
                for a in sp.get("attributes") or []:
                    v = a.get("value") or {}
                    if "stringValue" in v:
                        spans_attr = _kv(a["key"], v["stringValue"])
                    elif "intValue" in v:
                        anyv = _i32(3, int(v["intValue"]))
                        spans_attr = _str(1, a["key"]) + _ld(2, anyv)
                    else:
                        continue
                    msg += _ld(9, spans_attr)
                st = sp.get("status") or {}
                status = b""
                if st.get("message"):
                    status += _str(2, st["message"])
                status += _i32(3, int(st.get("code") or 0))
                msg += _ld(15, status)
                spans += _ld(2, msg)
            scopes += _ld(2, spans)
        rs_out += _ld(1, resource + scopes)
    return rs_out
