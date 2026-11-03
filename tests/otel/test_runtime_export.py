"""Native runtime OTLP export against a local collector (JSON OTLP/HTTP)."""

from __future__ import annotations

import json
import os
import sys
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler, HTTPServer

import pytest

from tests.helpers import free_port, start_server, stop, wait_ready

ROOT = os.path.join(os.path.dirname(__file__), "..", "..")


class Collector:
    def __init__(self) -> None:
        self.port = free_port()
        self.bodies: list[dict] = []
        parent = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:  # noqa: N802
                n = int(self.headers.get("Content-Length") or 0)
                raw = self.rfile.read(n)
                try:
                    parent.bodies.append(json.loads(raw.decode("utf-8")))
                except json.JSONDecodeError:
                    parent.bodies.append({"raw": raw.decode("utf-8", "replace")})
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(b"{}")

            def log_message(self, *_args) -> None:
                return

        self.httpd = HTTPServer(("127.0.0.1", self.port), Handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)

    def start(self) -> None:
        self.thread.start()

    def stop(self) -> None:
        self.httpd.shutdown()
        self.httpd.server_close()

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    def span_names(self) -> list[str]:
        names: list[str] = []
        for body in self.bodies:
            for rs in body.get("resourceSpans") or []:
                for ss in rs.get("scopeSpans") or []:
                    for span in ss.get("spans") or []:
                        names.append(span.get("name", ""))
        return names


def _run_agent(url: str) -> str:
    sys.path.insert(0, os.path.join(ROOT, "sdk", "python", "src"))
    os.environ["GOSHIKQ_RUNTIME_URL"] = url
    from goshikq_ai import Agent, Goal, Runtime, StateSnapshot, tool
    from goshikq_ai.models import MockModel, ModelTurn
    from goshikq_ai.state import SnapshotContext

    store = {"ok": False}

    @tool(effect="idempotent_write", idempotent=True, risk="low")
    def mark() -> dict:
        store["ok"] = True
        return {"ok": True}

    class S:
        def snapshot(self, context: SnapshotContext) -> StateSnapshot:
            return StateSnapshot(state=dict(store), version="1", source="mem")

        def version(self, resource_id: str) -> str | None:
            return "1"

    agent = Agent(
        name=f"otel-export-{uuid.uuid4().hex[:8]}",
        model=MockModel(script=[ModelTurn(tool_calls=[("mark", {})]), ModelTurn(content="done")]),
        tools=[mark],
    )
    goal = Goal(description="ok is true", success=lambda st: st.get("ok") is True)
    result = Runtime(agent, endpoint=url, state_provider=S()).run_sync(goal)
    assert result.verified, (result.status, result.verification_status, result.failure_reason)
    return str(result.run_id)


def test_runtime_exports_redacted_otlp_json():
    collector = Collector()
    collector.start()
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(
        port,
        {
            "GOSHIKQ_AUTH_MODE": "none",
            "GOSHIKQ_OTEL_EXPORTER_OTLP_ENDPOINT": collector.url,
            "OTEL_SERVICE_NAME": "goshikq-runtime-test",
            "GOSHIKQ_OTEL_TRACES_SAMPLER_ARG": "1",
        },
    )
    try:
        wait_ready(url)
        run_id = _run_agent(url)
        deadline = time.monotonic() + 8
        names: list[str] = []
        while time.monotonic() < deadline:
            names = collector.span_names()
            if "run.completed" in names or "tool.completed" in names or "goal.satisfied" in names:
                break
            time.sleep(0.2)
        assert names, f"collector received no spans for run {run_id}"
        text = json.dumps(collector.bodies)
        assert "sk-" not in text
        assert "axk_" not in text
        assert "payload" not in text
        assert any(
            n in names
            for n in ("run.started", "run.completed", "tool.started", "tool.completed", "verification.passed", "goal.satisfied")
        ), names
        assert "goshikq-runtime-test" in text
        assert run_id in text
    finally:
        stop(proc)
        collector.stop()
