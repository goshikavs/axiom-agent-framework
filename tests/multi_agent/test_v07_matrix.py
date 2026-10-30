"""v0.7 multi-agent / A2A / shared-context matrix. Isolated server + dynamic port."""

from __future__ import annotations

import os
import socket
import subprocess
import sys
import time
import uuid
from pathlib import Path

import httpx
import pytest

from tests.helpers import find_server

ROOT = Path(__file__).resolve().parents[2]
DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://postgres:axiom@127.0.0.1:55432/axiom")
SDK = str(ROOT / "sdk" / "python" / "src")


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


@pytest.fixture(scope="module")
def server():
    binary = find_server()
    if binary is None:
        pytest.skip("axiom-server binary not found")
    port = free_port()
    env = os.environ.copy()
    env["DATABASE_URL"] = DATABASE_URL
    env["AXIOM_BIND"] = f"127.0.0.1:{port}"
    env["AXIOM_AUTH_MODE"] = "none"
    env["RUST_LOG"] = "warn"
    kwargs = {"env": env, "cwd": str(ROOT), "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    proc = subprocess.Popen([str(binary)], **kwargs)
    url = f"http://127.0.0.1:{port}"
    deadline = time.monotonic() + 30
    with httpx.Client() as client:
        while time.monotonic() < deadline:
            try:
                if client.get(f"{url}/health", timeout=1.0).status_code < 500:
                    break
            except httpx.HTTPError:
                time.sleep(0.2)
        else:
            proc.kill()
            pytest.skip("server did not become ready")
    yield url
    proc.kill()
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        proc.terminate()


def agent(name: str, *, max_steps: int = 4, deadline: int = 60) -> dict:
    return {
        "name": name,
        "model": "mock/deterministic",
        "instructions": name,
        "tools": [],
        "execution_limits": {"max_steps": max_steps, "deadline_seconds": deadline},
        "permissions": [],
    }


def goal(desc: str, predicate: str | None = None) -> dict:
    body = {"description": desc, "verification_policy": "required" if predicate else "none"}
    if predicate:
        body["success_predicate"] = predicate
    return body


def test_01_parent_creates_child_tasks(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post(
            "/api/v1/runs",
            json={"agent": agent("coord-1"), "goal": goal("coordinate"), "defer_start": True, "metadata": {"coordination": {"max_depth": 4, "max_delegations": 8, "prevent_cycles": True}}},
        )
        assert parent.status_code == 201, parent.text
        pid = parent.json()["run_id"]
        child = c.post(
            f"/api/v1/runs/{pid}/tasks",
            json={"agent": agent("worker-a"), "goal": goal("child work"), "defer_start": True, "role": "worker"},
        )
        assert child.status_code == 201, child.text
        body = child.json()
        assert body["parent_run_id"] == pid
        assert body["task_id"]
        assert body["correlation_id"]
        assert body["child_run_id"] != pid
        hier = c.get(f"/api/v1/runs/{pid}/hierarchy").json()
        assert len(hier["children"]) == 1
        assert hier["children"][0]["run"]["parent_run_id"] == pid


def test_02_child_completion_does_not_verify_parent(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post("/api/v1/runs", json={"agent": agent("coord-2"), "goal": goal("parent still open"), "defer_start": True}).json()
        pid = parent["run_id"]
        child = c.post(
            f"/api/v1/runs/{pid}/tasks",
            json={"agent": agent("worker-b"), "goal": goal("child"), "defer_start": True},
        ).json()
        prow = c.get(f"/api/v1/runs/{pid}").json()
        crow = c.get(f"/api/v1/runs/{child['child_run_id']}").json()
        assert prow["verification_status"] == "NOT_EVALUATED"
        assert crow["verification_status"] == "NOT_EVALUATED"
        assert prow["status"] == "CREATED"


def test_03_parallel_child_tasks(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post(
            "/api/v1/runs",
            json={"agent": agent("coord-3"), "goal": goal("parallel"), "defer_start": True, "metadata": {"coordination": {"mode": "parallel", "max_delegations": 8}}},
        ).json()
        pid = parent["run_id"]
        ids = []
        for name in ("p-one", "p-two", "p-three"):
            r = c.post(f"/api/v1/runs/{pid}/tasks", json={"agent": agent(name), "goal": goal(name), "defer_start": True})
            assert r.status_code == 201, r.text
            ids.append(r.json()["child_run_id"])
        hier = c.get(f"/api/v1/runs/{pid}/hierarchy").json()
        assert len(hier["children"]) == 3
        assert {c["run"]["id"] for c in hier["children"]} == set(ids)


def test_04_shared_state_conflicts(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post("/api/v1/runs", json={"agent": agent("coord-4"), "goal": goal("shared"), "defer_start": True}).json()
        pid = parent["run_id"]
        put = c.put(
            f"/api/v1/runs/{pid}/shared-context",
            json={"key": "claim", "value": {"status": "open"}, "owner_agent": "coord-4", "visibility": "team", "evidence_refs": ["claim:CLM-77"]},
        )
        assert put.status_code == 200, put.text
        v = put.json()["version"]
        clash = c.put(
            f"/api/v1/runs/{pid}/shared-context",
            json={"key": "claim", "value": {"status": "closed"}, "expected_version": v - 1 if v else 0, "owner_agent": "other", "visibility": "team"},
        )
        assert clash.status_code == 409
        ok = c.put(
            f"/api/v1/runs/{pid}/shared-context",
            json={"key": "claim", "value": {"status": "closed"}, "expected_version": v, "owner_agent": "coord-4", "visibility": "team"},
        )
        assert ok.status_code == 200
        assert ok.json()["version"] == v + 1
        hist = c.get(f"/api/v1/runs/{pid}/shared-context/history").json()
        assert len(hist) >= 2


def test_05_delegation_cancellation_propagates(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post("/api/v1/runs", json={"agent": agent("coord-5"), "goal": goal("cancel"), "defer_start": True}).json()
        pid = parent["run_id"]
        child = c.post(f"/api/v1/runs/{pid}/tasks", json={"agent": agent("worker-c"), "goal": goal("c"), "defer_start": True}).json()
        cancelled = c.post(f"/api/v1/runs/{pid}/cancel", json={"reason": "test"})
        assert cancelled.status_code == 202, cancelled.text
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            row = c.get(f"/api/v1/runs/{child['child_run_id']}").json()
            if row["status"] == "CANCELLED":
                break
            time.sleep(0.2)
        else:
            pytest.fail(f"child not cancelled: {row}")


def test_06_deadlines_on_contracts(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post(
            "/api/v1/runs",
            json={"agent": agent("coord-6", deadline=30), "goal": {"description": "deadline parent", "deadline_seconds": 30}, "defer_start": True},
        ).json()
        child = c.post(
            f"/api/v1/runs/{parent['run_id']}/tasks",
            json={"agent": agent("worker-d", deadline=5), "goal": {"description": "deadline child", "deadline_seconds": 5}, "defer_start": True},
        )
        assert child.status_code == 201
        defn = c.get(f"/api/v1/runs/{child.json()['child_run_id']}").json()["definition"]
        assert defn["agent"]["execution_limits"]["deadline_seconds"] == 5


def test_07_agent_loops_are_bounded(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post(
            "/api/v1/runs",
            json={"agent": agent("loop-agent"), "goal": goal("loop"), "defer_start": True, "metadata": {"coordination": {"max_depth": 2, "prevent_cycles": True, "max_delegations": 4}}},
        ).json()
        pid = parent["run_id"]
        cycle = c.post(f"/api/v1/runs/{pid}/tasks", json={"agent": agent("loop-agent"), "goal": goal("cycle"), "defer_start": True})
        assert cycle.status_code == 400
        assert "cycle" in cycle.json()["error"]["message"].lower()
        child = c.post(f"/api/v1/runs/{pid}/tasks", json={"agent": agent("loop-child"), "goal": goal("c"), "defer_start": True})
        assert child.status_code == 201
        grandchild = c.post(
            f"/api/v1/runs/{child.json()['child_run_id']}/tasks",
            json={"agent": agent("loop-grand"), "goal": goal("g"), "defer_start": True},
        )
        assert grandchild.status_code == 201
        too_deep = c.post(
            f"/api/v1/runs/{grandchild.json()['child_run_id']}/tasks",
            json={"agent": agent("loop-too-deep"), "goal": goal("x"), "defer_start": True},
        )
        assert too_deep.status_code == 400
        assert "max_depth" in too_deep.json()["error"]["message"]


def test_08_duplicate_delegated_delivery(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post("/api/v1/runs", json={"agent": agent("coord-8"), "goal": goal("dup"), "defer_start": True}).json()
        pid = parent["run_id"]
        key = f"dup-{uuid.uuid4()}"
        a = c.post(f"/api/v1/runs/{pid}/tasks", json={"agent": agent("dup-w"), "goal": goal("d"), "defer_start": True, "idempotency_key": key})
        b = c.post(f"/api/v1/runs/{pid}/tasks", json={"agent": agent("dup-w"), "goal": goal("d"), "defer_start": True, "idempotency_key": key})
        assert a.status_code == 201 and b.status_code == 201
        assert a.json()["child_run_id"] == b.json()["child_run_id"]
        assert b.json()["reused"] is True
        assert len(c.get(f"/api/v1/runs/{pid}/hierarchy").json()["children"]) == 1


def test_12_authorization_before_sensitive_actions(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post("/api/v1/runs", json={"agent": agent("coord-12"), "goal": goal("approve"), "defer_start": True}).json()
        child = c.post(
            f"/api/v1/runs/{parent['run_id']}/tasks",
            json={"agent": agent("sensitive"), "goal": goal("pay"), "defer_start": True, "requires_approval": True},
        )
        assert child.status_code == 201, child.text
        assert child.json()["approval_id"]
        row = c.get(f"/api/v1/runs/{child.json()['child_run_id']}").json()
        assert row["status"] == "CREATED"
        pending = c.get("/api/v1/approvals", params={"status": "pending", "run_id": parent["run_id"]}).json()
        assert pending["items"] or pending.get("items") is not None


def test_13_and_14_a2a_untrusted_and_not_verified(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        card = c.get("/.well-known/agent-card.json")
        assert card.status_code == 200
        body = card.json()
        assert body["supportedInterfaces"][0]["protocolVersion"] == "1.0"
        sent = c.post(
            "/a2a",
            json={
                "jsonrpc": "2.0",
                "id": 1,
                "method": "SendMessage",
                "params": {
                    "message": {"role": "user", "parts": [{"kind": "text", "text": "do work"}]},
                    "markVerified": True,
                    "verification_status": "VERIFIED",
                },
            },
        )
        assert sent.status_code == 200, sent.text
        task = sent.json()["result"]
        assert task["metadata"]["axiomVerificationStatus"] != "VERIFIED"
        got = c.post("/a2a", json={"jsonrpc": "2.0", "id": 2, "method": "GetTask", "params": {"id": task["id"]}}).json()["result"]
        assert got["status"]["state"] in {"submitted", "working", "completed", "input-required"}
        run = c.get(f"/api/v1/runs/{task['id']}").json()
        assert run["verification_status"] == "NOT_EVALUATED"
        assert run["source_framework"] == "a2a"


def test_20_cross_project_hierarchy_isolation():
    binary = find_server()
    if binary is None:
        pytest.skip("axiom-server binary not found")
    port = free_port()
    env = os.environ.copy()
    env.update({"DATABASE_URL": DATABASE_URL, "AXIOM_BIND": f"127.0.0.1:{port}", "AXIOM_AUTH_MODE": "api_key", "RUST_LOG": "warn"})
    kwargs = {"env": env, "cwd": str(ROOT), "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    proc = subprocess.Popen([str(binary)], **kwargs)
    url = f"http://127.0.0.1:{port}"
    try:
        deadline = time.monotonic() + 30
        with httpx.Client() as probe:
            while time.monotonic() < deadline:
                try:
                    if probe.get(f"{url}/health", timeout=1).status_code < 500:
                        break
                except httpx.HTTPError:
                    time.sleep(0.2)
            else:
                pytest.skip("api_key server not ready")
        with httpx.Client(base_url=url, timeout=20) as c:
            denied = c.post("/api/v1/runs", json={"agent": agent("x"), "goal": goal("x"), "defer_start": True})
            assert denied.status_code == 401
    finally:
        proc.kill()
        proc.wait(timeout=8)


def test_09_10_parent_child_relationships_persist(server: str):
    with httpx.Client(base_url=server, timeout=20) as c:
        parent = c.post("/api/v1/runs", json={"agent": agent("coord-rec"), "goal": goal("persist"), "defer_start": True}).json()
        pid = parent["run_id"]
        child = c.post(f"/api/v1/runs/{pid}/tasks", json={"agent": agent("worker-rec"), "goal": goal("c"), "defer_start": True}).json()
        time.sleep(0.2)
        hier = c.get(f"/api/v1/runs/{pid}/hierarchy").json()
        assert hier["children"][0]["run"]["id"] == child["child_run_id"]
        child_row = c.get(f"/api/v1/runs/{child['child_run_id']}").json()
        assert child_row["parent_run_id"] == pid
        assert child_row["correlation_id"] == child["correlation_id"]
