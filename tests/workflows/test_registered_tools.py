"""Registered workflow TOOL nodes dispatch through the worker protocol; spawn fails closed."""

from __future__ import annotations

import os
import threading
import time
import uuid
from pathlib import Path

import httpx
import pytest

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = os.environ.get("AXIOM_RUNTIME_URL", "http://127.0.0.1:7400").rstrip("/")


def wait_status(client: httpx.Client, run_id: str, wanted: set[str], timeout: float = 45.0) -> dict:
    deadline = time.monotonic() + timeout
    last = {}
    while time.monotonic() < deadline:
        r = client.get(f"/api/v1/runs/{run_id}")
        r.raise_for_status()
        last = r.json()
        if last.get("status") in wanted:
            return last
        time.sleep(0.2)
    raise AssertionError(f"run {run_id} stayed {last.get('status')} not in {wanted}")


def linear_workflow(name: str, tool_node: dict, extra_nodes: list | None = None, extra_edges: list | None = None) -> dict:
    nodes = [
        {"id": "start", "kind": "start", "label": "start", "config": {}},
        tool_node,
        *(extra_nodes or []),
        {"id": "end", "kind": "end", "label": "end", "config": {}},
    ]
    edges = [{"id": "e1", "source": "start", "target": tool_node["id"]}, *(extra_edges or []), {"id": "e2", "source": tool_node["id"], "target": "end"}]
    return {"name": name, "version": 1, "nodes": nodes, "edges": edges, "max_steps": 12, "timeouts": {"run_seconds": 40}}


def publish_and_run(c: httpx.Client, definition: dict, input_payload: dict) -> str:
    created = c.post("/api/v1/workflows", json={"definition": definition, "draft": True})
    assert created.status_code in (200, 201), created.text
    wf_id = created.json()["id"]
    published = c.post(f"/api/v1/workflows/{wf_id}/publish")
    assert published.status_code == 200, published.text
    started = c.post(f"/api/v1/workflows/{wf_id}/run", json={"input": input_payload})
    assert started.status_code in (200, 201), started.text
    return started.json()["run_id"]


def serve_tool_worker(base: str, run_id: str, stop: threading.Event) -> None:
    with httpx.Client(base_url=base, timeout=25) as c:
        while not stop.is_set():
            r = c.post(f"/api/v1/runs/{run_id}/worker/poll", params={"timeout_ms": 1500})
            if r.status_code >= 400:
                time.sleep(0.1)
                continue
            body = r.json()
            req = body.get("request")
            if req:
                kind = req.get("kind")
                rid = req["request_id"]
                if kind == "tool.execute":
                    args = (req.get("payload") or {}).get("arguments") or {}
                    c.post(
                        f"/api/v1/runs/{run_id}/worker/respond",
                        json={"request_id": rid, "ok": True, "result": {"output": {"echoed": args, "worker": "py"}}},
                    )
                else:
                    c.post(
                        f"/api/v1/runs/{run_id}/worker/respond",
                        json={
                            "request_id": rid,
                            "ok": False,
                            "error": {"code": "not_implemented", "message": kind, "retryable": False},
                        },
                    )
            if body.get("terminal"):
                return


def test_registered_tool_executes_via_worker():
    with httpx.Client(base_url=RUNTIME, timeout=30) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        name = f"reg-tool-{uuid.uuid4().hex[:8]}"
        definition = linear_workflow(
            name,
            {
                "id": "tool",
                "kind": "tool",
                "label": "echo",
                "config": {"tool": "echo_box", "handler": "registered", "effect": "READ_ONLY", "worker_wait_seconds": 15},
                "input_schema": {"type": "object"},
            },
        )
        run_id = publish_and_run(c, definition, {"ping": "pong"})
        stop = threading.Event()
        t = threading.Thread(target=serve_tool_worker, args=(RUNTIME, run_id, stop), daemon=True)
        t.start()
        try:
            final = wait_status(c, run_id, {"COMPLETED", "FAILED", "UNVERIFIED", "TIMED_OUT"})
            assert final["status"] == "COMPLETED", final
            nodes = {n["node_id"]: n for n in c.get(f"/api/v1/runs/{run_id}/workflow-nodes").json()}
            assert nodes["tool"]["status"] == "completed"
            out = nodes["tool"].get("output") or {}
            assert out.get("registered") is True
        finally:
            stop.set()


def test_registered_tool_without_worker_fails_closed():
    with httpx.Client(base_url=RUNTIME, timeout=30) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        name = f"reg-noworker-{uuid.uuid4().hex[:8]}"
        definition = linear_workflow(
            name,
            {
                "id": "tool",
                "kind": "tool",
                "label": "echo",
                "config": {"tool": "echo_box", "handler": "registered", "worker_wait_seconds": 2},
                "input_schema": {"type": "object"},
            },
        )
        run_id = publish_and_run(c, definition, {"ping": "pong"})
        final = wait_status(c, run_id, {"FAILED", "TIMED_OUT", "COMPLETED"})
        assert final["status"] == "FAILED", final
        assert "WORKER_UNAVAILABLE" in (final.get("failure_reason") or "")


def test_spawned_agent_without_worker_is_unavailable():
    with httpx.Client(base_url=RUNTIME, timeout=30) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        name = f"spawn-noworker-{uuid.uuid4().hex[:8]}"
        definition = {
            "name": name,
            "version": 1,
            "nodes": [
                {"id": "start", "kind": "start", "label": "s", "config": {}},
                {"id": "agent", "kind": "agent", "label": "child", "config": {"agent": "spawned", "spawn": True, "worker_wait_seconds": 2}},
                {"id": "end", "kind": "end", "label": "e", "config": {}},
            ],
            "edges": [{"id": "e1", "source": "start", "target": "agent"}, {"id": "e2", "source": "agent", "target": "end"}],
            "max_steps": 8,
            "timeouts": {"run_seconds": 30},
        }
        run_id = publish_and_run(c, definition, {"x": 1})
        final = wait_status(c, run_id, {"FAILED", "TIMED_OUT", "COMPLETED"})
        assert final["status"] == "FAILED", final
        assert "WORKER_UNAVAILABLE" in (final.get("failure_reason") or "")
