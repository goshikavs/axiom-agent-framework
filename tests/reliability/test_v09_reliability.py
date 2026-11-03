"""Reliability probes: duplicate poll, concurrent approvals, SSE reconnect, tenant-scoped workflows."""

from __future__ import annotations

import os
import time
import uuid
from concurrent.futures import ThreadPoolExecutor

import httpx
import pytest

RUNTIME = os.environ.get("GOSHIKQ_RUNTIME_URL", "http://127.0.0.1:7400").rstrip("/")
WF = {
    "version": 1,
    "nodes": [
        {"id": "start", "kind": "start", "label": "s", "config": {}},
        {"id": "approve", "kind": "approval", "label": "ok", "config": {"policy": "required"}},
        {"id": "end", "kind": "end", "label": "e", "config": {}},
    ],
    "edges": [{"id": "e1", "source": "start", "target": "approve"}, {"id": "e2", "source": "approve", "target": "end"}],
    "max_steps": 8,
}


def wait_status(client: httpx.Client, run_id: str, wanted: set[str], timeout: float = 40.0) -> dict:
    deadline = time.monotonic() + timeout
    last = {}
    while time.monotonic() < deadline:
        r = client.get(f"/api/v1/runs/{run_id}")
        r.raise_for_status()
        last = r.json()
        if last.get("status") in wanted:
            return last
        time.sleep(0.15)
    raise AssertionError(f"run {run_id} stayed {last.get('status')} not in {wanted}")


def test_concurrent_approval_is_idempotent():
    with httpx.Client(base_url=RUNTIME, timeout=20) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        unique = dict(WF)
        unique["name"] = f"rel-appr-{uuid.uuid4().hex[:8]}"
        created = c.post("/api/v1/workflows", json={"definition": unique, "draft": True})
        assert created.status_code in (200, 201), created.text
        wf_id = created.json()["id"]
        assert c.post(f"/api/v1/workflows/{wf_id}/publish").status_code == 200
        started = c.post(f"/api/v1/workflows/{wf_id}/run", json={"input": {"n": 1}})
        assert started.status_code in (200, 201), started.text
        run_id = started.json()["run_id"]
        wait_status(c, run_id, {"WAITING_FOR_APPROVAL"})
        items = c.get("/api/v1/approvals", params={"status": "pending", "run_id": run_id}).json()["items"]
        assert items
        aid = items[0]["id"]

        def decide(_i: int) -> int:
            r = c.post(f"/api/v1/approvals/{aid}/decision", json={"decision": "approve", "reason": "race"})
            return r.status_code

        with ThreadPoolExecutor(max_workers=8) as pool:
            codes = list(pool.map(decide, range(8)))
        assert any(code < 300 for code in codes)
        assert all(code < 500 for code in codes)
        final = wait_status(c, run_id, {"COMPLETED", "FAILED"})
        assert final["status"] == "COMPLETED"


def test_duplicate_worker_poll_does_not_500():
    with httpx.Client(base_url=RUNTIME, timeout=15) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        fake = str(uuid.uuid4())
        r1 = c.post(f"/api/v1/runs/{fake}/worker/poll", params={"timeout_ms": 200})
        r2 = c.post(f"/api/v1/runs/{fake}/worker/poll", params={"timeout_ms": 200})
        assert r1.status_code in (404, 409)
        assert r2.status_code in (404, 409)


def test_sse_reconnect_replays_from_after():
    with httpx.Client(base_url=RUNTIME, timeout=20) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        unique = dict(WF)
        unique["name"] = f"rel-sse-{uuid.uuid4().hex[:8]}"
        unique["nodes"] = [
            {"id": "start", "kind": "start", "label": "s", "config": {}},
            {"id": "echo", "kind": "tool", "label": "echo", "config": {"tool": "echo", "handler": "echo"}},
            {"id": "end", "kind": "end", "label": "e", "config": {}},
        ]
        unique["edges"] = [{"id": "e1", "source": "start", "target": "echo"}, {"id": "e2", "source": "echo", "target": "end"}]
        created = c.post("/api/v1/workflows", json={"definition": unique, "draft": True})
        assert created.status_code in (200, 201), created.text
        wf_id = created.json()["id"]
        assert c.post(f"/api/v1/workflows/{wf_id}/publish").status_code == 200
        started = c.post(f"/api/v1/workflows/{wf_id}/run", json={"input": {"hello": True}})
        run_id = started.json()["run_id"]
        wait_status(c, run_id, {"COMPLETED", "FAILED", "UNVERIFIED"})
        first = c.get(f"/api/v1/runs/{run_id}/events", params={"limit": 5})
        first.raise_for_status()
        items = first.json()["items"]
        assert items
        after = items[0]["sequence"]
        replay = c.get(f"/api/v1/runs/{run_id}/events", params={"after": after, "limit": 50})
        replay.raise_for_status()
        for ev in replay.json()["items"]:
            assert ev["sequence"] > after
