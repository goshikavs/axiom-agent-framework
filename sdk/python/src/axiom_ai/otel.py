"""OTLP/HTTP helpers for exporting framework spans into Axiom.

Axiom does not control the traced runtime. Missing attributes stay omitted.
"""

from __future__ import annotations

import os
import secrets
import time
from dataclasses import dataclass, field
from typing import Any

from .client import RuntimeClient

MAPPING_VERSION = "otel-genai-v1"


def _now_ns() -> int:
    return time.time_ns()


def _hex(n: int) -> str:
    return secrets.token_hex(n)


@dataclass
class Span:
    trace_id: str
    span_id: str
    name: str
    parent_span_id: str | None = None
    start_unix_nano: int = 0
    end_unix_nano: int = 0
    kind: int = 1
    status_code: int = 1
    status_message: str | None = None
    attributes: dict[str, Any] = field(default_factory=dict)


def _attr_value(v: Any) -> dict[str, Any]:
    if isinstance(v, bool):
        return {"boolValue": v}
    if isinstance(v, int) and not isinstance(v, bool):
        return {"intValue": str(v)}
    if isinstance(v, float):
        return {"doubleValue": v}
    return {"stringValue": str(v)}


def spans_to_otlp_json(
    spans: list[Span],
    *,
    service_name: str,
    framework: str,
    extra_resource: dict[str, Any] | None = None,
) -> dict[str, Any]:
    resource_attrs = [
        {"key": "service.name", "value": {"stringValue": service_name}},
        {"key": "axiom.source_framework", "value": {"stringValue": framework}},
    ]
    for k, v in (extra_resource or {}).items():
        resource_attrs.append({"key": k, "value": _attr_value(v)})
    encoded = []
    for s in spans:
        item: dict[str, Any] = {
            "traceId": s.trace_id,
            "spanId": s.span_id,
            "name": s.name,
            "kind": s.kind,
            "startTimeUnixNano": str(s.start_unix_nano),
            "endTimeUnixNano": str(s.end_unix_nano),
            "status": {"code": s.status_code, **({"message": s.status_message} if s.status_message else {})},
            "attributes": [{"key": k, "value": _attr_value(v)} for k, v in s.attributes.items()],
        }
        if s.parent_span_id:
            item["parentSpanId"] = s.parent_span_id
        encoded.append(item)
    return {
        "resourceSpans": [
            {
                "resource": {"attributes": resource_attrs},
                "scopeSpans": [{"scope": {"name": f"axiom.{framework}", "version": MAPPING_VERSION}, "spans": encoded}],
            }
        ]
    }


class OtlpTracer:
    """In-process span recorder that flushes as OTLP/JSON."""

    def __init__(self, *, service_name: str, framework: str, client: RuntimeClient | None = None) -> None:
        self.service_name = service_name
        self.framework = framework
        self.client = client or RuntimeClient()
        self._spans: list[Span] = []
        self.trace_id = _hex(16)

    def start(self, name: str, *, parent: Span | None = None, attributes: dict[str, Any] | None = None) -> Span:
        span = Span(
            trace_id=self.trace_id,
            span_id=_hex(8),
            name=name,
            parent_span_id=parent.span_id if parent else None,
            start_unix_nano=_now_ns(),
            attributes=dict(attributes or {}),
        )
        return span

    def end(self, span: Span, *, status_code: int = 1, error: str | None = None, attributes: dict[str, Any] | None = None) -> Span:
        if attributes:
            span.attributes.update(attributes)
        span.end_unix_nano = _now_ns()
        span.status_code = status_code
        span.status_message = error
        self._spans.append(span)
        return span

    def payload(self) -> dict[str, Any]:
        return spans_to_otlp_json(self._spans, service_name=self.service_name, framework=self.framework)

    async def flush(self) -> Any:
        body = self.payload()
        path = os.environ.get("AXIOM_OTLP_PATH", "/v1/traces")
        return await self.client._request("POST", path, json=body)
