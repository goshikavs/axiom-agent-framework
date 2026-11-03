"""OIDC JWT + membership: Tenant A cannot read or mutate Tenant B."""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import os
import socket
import subprocess
import time
import uuid
from pathlib import Path

import httpx
import pytest

from tests.helpers import find_server

ROOT = Path(__file__).resolve().parents[2]
DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://postgres:goshikq@127.0.0.1:55432/goshikq")
HMAC = "v09-test-hmac-secret-at-least-32b!!"
ISSUER = "http://127.0.0.1/idp"
AUD = "goshikq"


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def mint(sub: str, extra: dict | None = None) -> str:
    header = b64url(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    payload = {
        "sub": sub,
        "iss": ISSUER,
        "aud": AUD,
        "exp": int(time.time()) + 3600,
        "nbf": int(time.time()) - 5,
    }
    if extra:
        payload.update(extra)
    body = b64url(json.dumps(payload, separators=(",", ":")).encode())
    sig = hmac.new(HMAC.encode(), f"{header}.{body}".encode(), hashlib.sha256).digest()
    return f"{header}.{body}.{b64url(sig)}"


def start_server(port: int) -> subprocess.Popen:
    binary = find_server()
    if binary is None:
        pytest.skip("goshikq-server binary not found")
    env = os.environ.copy()
    env.update(
        {
            "DATABASE_URL": DATABASE_URL,
            "GOSHIKQ_BIND": f"127.0.0.1:{port}",
            "GOSHIKQ_AUTH_MODE": "oidc",
            "GOSHIKQ_OIDC_ISSUER": ISSUER,
            "GOSHIKQ_OIDC_AUDIENCE": AUD,
            "GOSHIKQ_OIDC_HMAC_SECRET": HMAC,
            "GOSHIKQ_OIDC_ALLOW_TEST_HMAC": "true",
            "GOSHIKQ_OIDC_BOOTSTRAP_OWNER": "alice",
            "RUST_LOG": env.get("RUST_LOG", "warn"),
        }
    )
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


@pytest.fixture
def oidc_runtime():
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port)
    try:
        wait_ready(url)
        yield url
    finally:
        stop(proc)


def auth(sub: str, project: str | None = None) -> dict[str, str]:
    headers = {"authorization": f"Bearer {mint(sub)}"}
    if project:
        headers["x-goshikq-project-id"] = project
    return headers


def test_alice_cannot_read_bob_project(oidc_runtime: str):
    url = oidc_runtime
    bob = f"iso-bob-{uuid.uuid4().hex[:8]}"
    with httpx.Client(base_url=url, timeout=20) as c:
        who = c.get("/api/v1/whoami", headers=auth("alice"))
        assert who.status_code == 200, who.text
        assert who.json()["role"] == "owner"

        slug = f"tenant-b-{uuid.uuid4().hex[:8]}"
        created = c.post("/api/v1/projects", json={"name": "tenant-b", "slug": slug}, headers=auth("alice"))
        assert created.status_code == 201, created.text
        project_b = created.json()["id"]

        member = c.post(
            "/api/v1/members",
            json={"subject": bob, "role": "owner"},
            headers=auth("alice", project_b),
        )
        assert member.status_code == 200, member.text

        wf = {
            "name": f"iso-b-{uuid.uuid4().hex[:6]}",
            "version": 1,
            "nodes": [
                {"id": "start", "kind": "start", "label": "s", "config": {}},
                {"id": "end", "kind": "end", "label": "e", "config": {}},
            ],
            "edges": [{"id": "e1", "source": "start", "target": "end"}],
            "max_steps": 8,
        }
        created_wf = c.post("/api/v1/workflows", json={"definition": wf, "draft": True}, headers=auth("alice", project_b))
        assert created_wf.status_code in (200, 201), created_wf.text
        wf_id = created_wf.json()["id"]
        pub = c.post(f"/api/v1/workflows/{wf_id}/publish", headers=auth("alice", project_b))
        assert pub.status_code == 200, pub.text
        started = c.post(f"/api/v1/workflows/{wf_id}/run", json={"input": {"k": 1}}, headers=auth("alice", project_b))
        assert started.status_code in (200, 201), started.text
        run_id = started.json()["run_id"]

        removed = c.request("DELETE", f"/api/v1/members/alice", headers=auth("alice", project_b))
        assert removed.status_code in (204, 200), removed.text

        # Bob is not a member of the default project.
        denied_default = c.get("/api/v1/runs", headers=auth(bob))
        assert denied_default.status_code == 403

        # Alice on default project must not see B's run by guessed id.
        guessed = c.get(f"/api/v1/runs/{run_id}", headers=auth("alice"))
        assert guessed.status_code in (403, 404), guessed.text
        # Alice claiming project B after membership removal.
        cross = c.get(f"/api/v1/runs/{run_id}", headers=auth("alice", project_b))
        assert cross.status_code == 403, cross.text
        stream = c.get(f"/api/v1/runs/{run_id}/events", headers=auth("alice"))
        assert stream.status_code in (403, 404)
        sse = c.get(f"/api/v1/runs/{run_id}/events/stream", headers=auth("alice"))
        assert sse.status_code in (403, 404)
        ev = c.get(f"/api/v1/runs/{run_id}/evidence", headers=auth("alice"))
        assert ev.status_code in (403, 404)
        a2a = c.post(
            "/a2a",
            json={"jsonrpc": "2.0", "id": 1, "method": "tasks/get", "params": {"id": run_id}},
            headers=auth("alice"),
        )
        assert a2a.status_code in (200, 401, 403, 404)
        if a2a.status_code == 200 and "result" in a2a.json():
            pytest.fail("default-project caller must not read tenant B A2A task")

        bob_run = c.get(f"/api/v1/runs/{run_id}", headers=auth(bob, project_b))
        assert bob_run.status_code == 200, bob_run.text

        header = b64url(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
        payload = b64url(json.dumps({"sub": "alice", "iss": "https://evil.example", "aud": AUD, "exp": int(time.time()) + 60}).encode())
        sig = hmac.new(HMAC.encode(), f"{header}.{payload}".encode(), hashlib.sha256).digest()
        evil = f"{header}.{payload}.{b64url(sig)}"
        rejected = c.get("/api/v1/whoami", headers={"authorization": f"Bearer {evil}"})
        assert rejected.status_code == 401

        unauth = c.get("/api/v1/runs")
        assert unauth.status_code == 401


def test_operator_api_key_cannot_approve(oidc_runtime: str):
    url = oidc_runtime
    with httpx.Client(base_url=url, timeout=20) as c:
        key = c.post("/api/v1/api-keys", json={"name": "ops", "role": "operator"}, headers=auth("alice"))
        assert key.status_code == 201, key.text
        token = key.json()["token"]
        headers = {"authorization": f"Bearer {token}"}
        # No pending approval is required: the role check fires before lookup when we hit decide with a random id.
        fake = str(uuid.uuid4())
        decided = c.post(f"/api/v1/approvals/{fake}/decision", json={"decision": "approve"}, headers=headers)
        assert decided.status_code == 403, decided.text
