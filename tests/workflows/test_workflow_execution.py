"""Execute a published workflow through the Rust runtime (not Studio)."""

from __future__ import annotations

import json
import os
import time
from pathlib import Path

import httpx
import pytest

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = os.environ.get("AXIOM_RUNTIME_URL", "http://127.0.0.1:7400").rstrip("/")
WF = json.loads((ROOT / "examples/visual-workflow/parallel-approval.json").read_text(encoding="utf-8"))


def wait_status(client: httpx.Client, run_id: str, wanted: set[str], timeout: float = 60.0) -> dict:
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


def test_parallel_approval_workflow_executes():
    with httpx.Client(base_url=RUNTIME, timeout=30) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        unique = dict(WF)
        unique["name"] = f"parallel-approval-{int(time.time())}"
        created = c.post("/api/v1/workflows", json={"definition": unique, "draft": True})
        assert created.status_code in (200, 201), created.text
        wf_id = created.json()["id"]
        published = c.post(f"/api/v1/workflows/{wf_id}/publish")
        assert published.status_code == 200, published.text
        version = published.json()["published_version"]
        started = c.post(f"/api/v1/workflows/{wf_id}/run", json={"input": {"order_id": "ORD-1"}, "version": version})
        assert started.status_code in (200, 201), started.text
        run_id = started.json()["run_id"]
        waiting = wait_status(c, run_id, {"WAITING_FOR_APPROVAL", "COMPLETED", "FAILED"})
        if waiting["status"] == "WAITING_FOR_APPROVAL":
            approvals = c.get("/api/v1/approvals", params={"status": "pending", "run_id": run_id})
            approvals.raise_for_status()
            items = approvals.json().get("items") or approvals.json()
            if isinstance(items, dict):
                items = items.get("items", [])
            assert items, "expected a pending workflow approval"
            aid = items[0]["id"]
            decided = c.post(f"/api/v1/approvals/{aid}/decision", json={"decision": "approve", "reason": "test"})
            assert decided.status_code < 300, decided.text
            waiting = wait_status(c, run_id, {"COMPLETED", "FAILED", "UNVERIFIED"})
        assert waiting["status"] == "COMPLETED", waiting
        nodes = c.get(f"/api/v1/runs/{run_id}/workflow-nodes")
        nodes.raise_for_status()
        by_id = {n["node_id"]: n["status"] for n in nodes.json()}
        for required in ("start", "fork", "inspect", "eligible", "join", "approve", "verify", "end"):
            assert by_id.get(required) == "completed", by_id
        # Version pinning: editing the draft must not change the in-flight/published snapshot used by this run.
        edited = dict(unique)
        edited["description"] = "mutated after publish"
        c.put(f"/api/v1/workflows/{wf_id}", json={"definition": edited, "draft": True}).raise_for_status()
        run = c.get(f"/api/v1/runs/{run_id}").json()
        pinned = run["definition"]["metadata"]["workflow"]["version"]
        assert pinned == version


def test_workflow_can_be_cancelled():
    with httpx.Client(base_url=RUNTIME, timeout=30) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        unique = dict(WF)
        unique["name"] = f"cancel-approval-{int(time.time())}"
        created = c.post("/api/v1/workflows", json={"definition": unique, "draft": True})
        assert created.status_code in (200, 201), created.text
        wf_id = created.json()["id"]
        assert c.post(f"/api/v1/workflows/{wf_id}/publish").status_code == 200
        started = c.post(f"/api/v1/workflows/{wf_id}/run", json={"input": {"order_id": "CXL-1"}})
        assert started.status_code in (200, 201), started.text
        run_id = started.json()["run_id"]
        wait_status(c, run_id, {"WAITING_FOR_APPROVAL"})
        cancelled = c.post(f"/api/v1/runs/{run_id}/cancel", json={"reason": "workflow-test"})
        assert cancelled.status_code < 300, cancelled.text
        final = wait_status(c, run_id, {"CANCELLED", "FAILED"})
        assert final["status"] == "CANCELLED"


def test_invalid_workflow_is_rejected():
    bad = {"name": "bad", "version": 1, "nodes": [], "edges": [], "max_steps": 1}
    with httpx.Client(base_url=RUNTIME, timeout=15) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        v = c.post("/api/v1/workflows/validate", json={"definition": bad})
        v.raise_for_status()
        body = v.json()
        assert body["valid"] is False
        codes = {i["code"] for i in body["issues"]}
        assert "start_node_count" in codes
        assert "missing_end_node" in codes
        denied = c.post("/api/v1/workflows", json={"definition": bad, "draft": False})
        assert denied.status_code == 422
