"""Administrative retention purge skips active runs and parent-of-active rows."""

from __future__ import annotations

import os
import sys
import time
import uuid

import httpx
import pytest

from tests.helpers import free_port, start_server, stop, wait_ready

ROOT_EXAMPLES = os.path.join(os.path.dirname(__file__), "..", "..")


@pytest.fixture
def runtime():
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port, {"GOSHIKQ_AUTH_MODE": "none", "GOSHIKQ_RETENTION_DAYS": "30"})
    try:
        wait_ready(url)
        yield url
    finally:
        stop(proc)


def _complete_echo(url: str) -> str:
    sys.path.insert(0, os.path.join(ROOT_EXAMPLES, "sdk", "python", "src"))
    os.environ["GOSHIKQ_RUNTIME_URL"] = url
    from goshikq_ai import Agent, Goal, Runtime, StateSnapshot, tool
    from goshikq_ai.models import MockModel, ModelTurn
    from goshikq_ai.state import SnapshotContext

    store = {"n": 0}

    @tool(effect="idempotent_write", idempotent=True, risk="low")
    def bump() -> dict:
        store["n"] += 1
        return {"n": store["n"]}

    class S:
        def snapshot(self, context: SnapshotContext) -> StateSnapshot:
            return StateSnapshot(state=dict(store), version=str(store["n"]), source="mem")

        def version(self, resource_id: str) -> str | None:
            return str(store["n"])

    agent = Agent(
        name=f"retention-{uuid.uuid4().hex[:8]}",
        model=MockModel(script=[ModelTurn(tool_calls=[("bump", {})]), ModelTurn(content="ok")]),
        tools=[bump],
    )
    goal = Goal(description="n is 1", success=lambda st: st.get("n") == 1)
    result = Runtime(agent, endpoint=url, state_provider=S()).run_sync(goal)
    assert result.verified, (result.status, result.verification_status, result.failure_reason)
    return str(result.run_id)


def test_purge_dry_run_then_targeted_delete(runtime):
    url = runtime
    run_id = _complete_echo(url)
    with httpx.Client() as c:
        dry = c.post(
            f"{url}/api/v1/admin/retention/purge",
            json={"dry_run": True, "older_than_days": 0, "only_run_ids": [run_id]},
            timeout=10,
        )
        assert dry.status_code == 200, dry.text
        body = dry.json()
        assert body["dry_run"] is True
        assert run_id in body["run_ids"]
        still = c.get(f"{url}/api/v1/runs/{run_id}", timeout=5)
        assert still.status_code == 200
        gone = c.post(
            f"{url}/api/v1/admin/retention/purge",
            json={"dry_run": False, "older_than_days": 0, "only_run_ids": [run_id]},
            timeout=10,
        )
        assert gone.status_code == 200, gone.text
        assert gone.json()["deleted"] == 1
        missing = c.get(f"{url}/api/v1/runs/{run_id}", timeout=5)
        assert missing.status_code == 404


def test_purge_does_not_delete_active(runtime):
    url = runtime
    fake = str(uuid.uuid4())
    with httpx.Client() as c:
        r = c.post(
            f"{url}/api/v1/admin/retention/purge",
            json={"dry_run": True, "older_than_days": 0, "only_run_ids": [fake]},
            timeout=10,
        )
        assert r.status_code == 200
        assert r.json()["run_ids"] == []
        assert r.json()["deleted"] == 0
