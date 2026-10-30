"""Restart axiom-server while a published workflow is waiting for approval.

Uses a dedicated subprocess so it does not depend on the long-lived :7400 runtime.
Completed nodes must remain completed; the approval waiter must resume.
"""

from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path

import httpx
import pytest

ROOT = Path(__file__).resolve().parents[2]
WF = json.loads((ROOT / "examples" / "visual-workflow" / "parallel-approval.json").read_text(encoding="utf-8"))
DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://postgres:axiom@127.0.0.1:55432/axiom")


from tests.helpers import find_server


def postgres_up() -> bool:
    try:
        import urllib.parse

        u = urllib.parse.urlparse(DATABASE_URL)
        with socket.create_connection((u.hostname or "127.0.0.1", u.port or 5432), timeout=1.5):
            return True
    except OSError:
        return False


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def start_server(port: int) -> subprocess.Popen:
    binary = find_server()
    if binary is None:
        pytest.skip("axiom-server binary not found")
    env = os.environ.copy()
    env["DATABASE_URL"] = DATABASE_URL
    env["AXIOM_BIND"] = f"127.0.0.1:{port}"
    env["AXIOM_AUTH_MODE"] = "none"
    env["AXIOM_LEASE_TTL_SECS"] = "30"
    env.pop("AXIOM_CRASH_AT", None)
    kwargs: dict = {"env": env, "cwd": str(ROOT), "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    return subprocess.Popen([str(binary)], **kwargs)


def wait_ready(url: str, timeout: float = 30.0) -> None:
    deadline = time.monotonic() + timeout
    with httpx.Client(timeout=2.0) as client:
        while time.monotonic() < deadline:
            try:
                r = client.get(f"{url}/health")
                if r.status_code < 500:
                    return
            except httpx.HTTPError:
                time.sleep(0.2)
        raise TimeoutError(f"server at {url} did not become ready")


@pytest.mark.skipif(not postgres_up(), reason="PostgreSQL not reachable")
def test_workflow_approval_survives_process_restart() -> None:
    if find_server() is None:
        pytest.skip("axiom-server binary not found")
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port)
    try:
        wait_ready(url)
        unique = dict(WF)
        unique["name"] = f"wf-recovery-{int(time.time())}"
        with httpx.Client(base_url=url, timeout=30) as c:
            created = c.post("/api/v1/workflows", json={"definition": unique, "draft": True})
            assert created.status_code in (200, 201), created.text
            wf_id = created.json()["id"]
            published = c.post(f"/api/v1/workflows/{wf_id}/publish")
            assert published.status_code == 200, published.text
            started = c.post(f"/api/v1/workflows/{wf_id}/run", json={"input": {"order_id": "REC-1"}})
            assert started.status_code in (200, 201), started.text
            run_id = started.json()["run_id"]
            deadline = time.monotonic() + 45
            status = None
            while time.monotonic() < deadline:
                status = c.get(f"/api/v1/runs/{run_id}").json().get("status")
                if status == "WAITING_FOR_APPROVAL":
                    break
                if status in {"FAILED", "CANCELLED", "COMPLETED"}:
                    pytest.fail(f"run reached {status} before approval wait")
                time.sleep(0.2)
            assert status == "WAITING_FOR_APPROVAL", status
            before = {n["node_id"]: n["status"] for n in c.get(f"/api/v1/runs/{run_id}/workflow-nodes").json()}
            assert before.get("start") == "completed"

        proc.kill()
        proc.wait(timeout=15)
        proc = start_server(port)
        wait_ready(url, timeout=45)

        with httpx.Client(base_url=url, timeout=30) as c:
            rec = c.get(f"/api/v1/runs/{run_id}/recovery")
            assert rec.status_code == 200, rec.text
            after = {n["node_id"]: n["status"] for n in c.get(f"/api/v1/runs/{run_id}/workflow-nodes").json()}
            assert after.get("start") == "completed"
            deadline = time.monotonic() + 30
            approval_id = None
            while time.monotonic() < deadline:
                items = c.get("/api/v1/approvals", params={"status": "pending", "run_id": run_id}).json()
                if isinstance(items, dict):
                    items = items.get("items") or []
                if items:
                    approval_id = items[0]["id"]
                    break
                time.sleep(0.4)
            assert approval_id, "approval did not survive workflow restart"
            dec = c.post(
                f"/api/v1/approvals/{approval_id}/decision",
                json={"decision": "approve", "reason": "workflow-recovery"},
                headers={"x-axiom-actor": "workflow-recovery"},
            )
            assert dec.status_code < 300, dec.text
            deadline = time.monotonic() + 60
            last = {}
            while time.monotonic() < deadline:
                last = c.get(f"/api/v1/runs/{run_id}").json()
                if last.get("status") in {"COMPLETED", "FAILED", "UNVERIFIED", "CANCELLED"}:
                    break
                time.sleep(0.3)
            assert last.get("status") == "COMPLETED", last
            nodes = {n["node_id"]: n["status"] for n in c.get(f"/api/v1/runs/{run_id}/workflow-nodes").json()}
            for required in ("start", "fork", "inspect", "eligible", "join", "approve", "verify", "end"):
                assert nodes.get(required) == "completed", nodes
    finally:
        if proc.poll() is None:
            proc.kill()


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
