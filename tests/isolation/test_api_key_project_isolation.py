"""Paired two-project API-key isolation for REST and OTLP."""

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
DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://postgres:goshikq@127.0.0.1:55432/goshikq")


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def start_server(port: int, auth: str) -> subprocess.Popen:
    binary = find_server()
    if binary is None:
        pytest.skip("goshikq-server binary not found")
    env = os.environ.copy()
    env["DATABASE_URL"] = DATABASE_URL
    env["GOSHIKQ_BIND"] = f"127.0.0.1:{port}"
    env["GOSHIKQ_AUTH_MODE"] = auth
    env["RUST_LOG"] = env.get("RUST_LOG", "warn")
    kwargs: dict = {"env": env, "cwd": str(ROOT), "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    return subprocess.Popen([str(binary)], **kwargs)


def wait_ready(url: str, timeout: float = 30.0) -> None:
    deadline = time.monotonic() + timeout
    with httpx.Client() as client:
        while time.monotonic() < deadline:
            try:
                r = client.get(f"{url}/health", timeout=1.0)
                if r.status_code < 500:
                    return
            except httpx.HTTPError:
                time.sleep(0.2)
        raise TimeoutError(f"server at {url} did not become ready")


def stop(proc: subprocess.Popen) -> None:
    if proc.poll() is not None:
        return
    proc.kill()
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        proc.terminate()


def otlp_body(agent: str, trace_id: str) -> dict:
    now = str(int(time.time() * 1e9))
    return {
        "resourceSpans": [
            {
                "resource": {
                    "attributes": [
                        {"key": "service.name", "value": {"stringValue": agent}},
                        {"key": "goshikq.source_framework", "value": {"stringValue": "otel"}},
                    ]
                },
                "scopeSpans": [
                    {
                        "scope": {"name": "goshikq.isolation", "version": "otel-genai-v1"},
                        "spans": [
                            {
                                "traceId": trace_id,
                                "spanId": uuid.uuid4().hex[:16],
                                "name": "invoke_agent",
                                "kind": 1,
                                "startTimeUnixNano": now,
                                "endTimeUnixNano": now,
                                "status": {"code": 1},
                                "attributes": [
                                    {"key": "gen_ai.operation.name", "value": {"stringValue": "invoke_agent"}},
                                    {"key": "gen_ai.agent.name", "value": {"stringValue": agent}},
                                ],
                            }
                        ],
                    }
                ],
            }
        ]
    }


@pytest.fixture
def paired_keys():
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port, "none")
    try:
        wait_ready(url)
        slug = f"iso-{uuid.uuid4().hex[:8]}"
        with httpx.Client(base_url=url, timeout=15) as c:
            proj = c.post("/api/v1/projects", json={"name": "isolation-b", "slug": slug})
            proj.raise_for_status()
            project_b = proj.json()["id"]
            key_a = c.post("/api/v1/api-keys", json={"name": "iso-a"}).json()
            key_b = c.post("/api/v1/api-keys", json={"name": "iso-b", "project_id": project_b}).json()
            assert key_a["token"].startswith("axk_")
            assert key_b["project_id"] == project_b
            assert key_a["project_id"] != key_b["project_id"]
    finally:
        stop(proc)

    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port, "api_key")
    wait_ready(url)
    yield {"url": url, "key_a": key_a, "key_b": key_b, "proc": proc}
    stop(proc)


def test_otlp_and_rest_do_not_cross_projects(paired_keys):
    url = paired_keys["url"]
    key_a = paired_keys["key_a"]["token"]
    key_b = paired_keys["key_b"]["token"]
    project_b = paired_keys["key_b"]["project_id"]
    trace = uuid.uuid4().hex
    headers_a = {"authorization": f"Bearer {key_a}", "content-type": "application/json"}
    headers_b = {"authorization": f"Bearer {key_b}", "content-type": "application/json"}

    with httpx.Client(base_url=url, timeout=15) as c:
        r = c.post("/v1/traces", json=otlp_body("iso-agent-a", trace), headers=headers_a)
        assert r.status_code < 300, r.text

        mismatch = dict(headers_a)
        mismatch["x-goshikq-project-id"] = project_b
        denied = c.post("/v1/traces", json=otlp_body("iso-agent-cross", uuid.uuid4().hex), headers=mismatch)
        assert denied.status_code == 403

        listed_b = c.get("/api/v1/runs", params={"limit": 50}, headers=headers_b)
        listed_b.raise_for_status()
        for item in listed_b.json().get("items", []):
            assert item.get("trace_id") != trace

        listed_a = c.get("/api/v1/runs", params={"limit": 50}, headers=headers_a)
        listed_a.raise_for_status()
        run = next((i for i in listed_a.json().get("items", []) if i.get("trace_id") == trace), None)
        assert run is not None, "project A must see its own OTLP run"

        missing = c.get(f"/api/v1/runs/{run['id']}", headers=headers_b)
        assert missing.status_code in (403, 404)

        unauth = c.get("/api/v1/runs")
        assert unauth.status_code == 401

        hier = c.get(f"/api/v1/runs/{run['id']}/hierarchy", headers=headers_b)
        assert hier.status_code in (403, 404)
        shared = c.get(f"/api/v1/runs/{run['id']}/shared-context", headers=headers_b)
        assert shared.status_code in (403, 404)
        a2a = c.post("/a2a", json={"jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": {"id": run["id"]}}, headers=headers_b)
        assert a2a.status_code in (200, 401, 403, 404)
        if a2a.status_code == 200 and "result" in a2a.json():
            pytest.fail("project B must not read project A's A2A task")
        wf = c.get("/api/v1/workflows", headers=headers_b)
        wf.raise_for_status()
        for item in wf.json().get("items", []):
            if item.get("project_id"):
                assert item["project_id"] == project_b


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-q"]))
