"""Process-crash tests for durable recovery (Goshikq v0.2).

Launches a real `goshikq-server` subprocess, kills it at GOSHIKQ_CRASH_AT barriers
(exit 97), restarts it, and asserts the in-process worker resumes without
duplicate refunds.

Requires:
  DATABASE_URL pointing at a reachable PostgreSQL (not embedded)
  an `goshikq-server` binary on PATH or under target/

Windows ARM64: the binary is the x86_64 MSVC build (`scripts/with-msvc.ps1`).
These tests spawn that same binary; they are not excluded on ARM64 hosts.
"""

from __future__ import annotations

import asyncio
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
sys.path.insert(0, str(ROOT / "examples" / "retail-refund"))
sys.path.insert(0, str(ROOT / "sdk" / "python" / "src"))

from goshikq_ai import Agent, Goal, Runtime  # noqa: E402
from goshikq_ai.models import MockModel, ModelTurn  # noqa: E402
from store import Fault, demo_store  # noqa: E402

from main import StoreProvider, build_agent, build_goal  # noqa: E402

DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://postgres:goshikq@127.0.0.1:55432/goshikq")


from tests.helpers import find_server


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


def start_server(port: int, crash_at: str | None = None) -> subprocess.Popen:
    binary = find_server()
    if binary is None:
        pytest.skip("goshikq-server binary not found; build with scripts/with-msvc.ps1 cargo ... --bin goshikq-server")
    env = os.environ.copy()
    env["DATABASE_URL"] = DATABASE_URL
    env["GOSHIKQ_BIND"] = f"127.0.0.1:{port}"
    env["GOSHIKQ_AUTH_MODE"] = "none"
    env["GOSHIKQ_CAPTURE_MODEL_IO"] = "full"
    env["GOSHIKQ_LEASE_TTL_SECS"] = "8"
    env["RUST_LOG"] = env.get("RUST_LOG", "warn")
    if crash_at:
        env["GOSHIKQ_CRASH_AT"] = crash_at
    else:
        env.pop("GOSHIKQ_CRASH_AT", None)
    kwargs: dict = {"env": env, "cwd": str(ROOT), "stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
    if os.name == "nt":
        kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    return subprocess.Popen([str(binary)], **kwargs)


async def wait_ready(url: str, timeout: float = 30.0) -> None:
    deadline = time.monotonic() + timeout
    async with httpx.AsyncClient() as client:
        while time.monotonic() < deadline:
            try:
                r = await client.get(f"{url}/health", timeout=1.0)
                if r.status_code < 500:
                    return
            except httpx.HTTPError:
                await asyncio.sleep(0.2)
        raise TimeoutError(f"server at {url} did not become ready")


async def wait_exit(proc: subprocess.Popen, timeout: float = 60.0) -> int:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        code = proc.poll()
        if code is not None:
            return code
        await asyncio.sleep(0.1)
    proc.kill()
    raise TimeoutError("server did not crash at the requested barrier")


def postgres_up() -> bool:
    if DATABASE_URL in ("embedded", ""):
        return False
    try:
        import urllib.parse

        u = urllib.parse.urlparse(DATABASE_URL)
        with socket.create_connection((u.hostname or "127.0.0.1", u.port or 5432), timeout=1.5):
            return True
    except OSError:
        return False


@pytest.fixture(scope="module")
def require_env() -> None:
    if not postgres_up():
        pytest.skip(f"PostgreSQL not reachable at {DATABASE_URL}")
    if find_server() is None:
        pytest.skip("goshikq-server binary not found")


async def refund_run(endpoint: str, fault: str = Fault.DELAYED_SETTLEMENT, wait_seconds: str = "25"):
    os.environ["GOSHIKQ_RUNTIME_URL"] = endpoint
    os.environ["GOSHIKQ_REFUND_ORDER"] = "ORD-2048"
    os.environ["GOSHIKQ_REFUND_FAULT"] = fault
    os.environ["GOSHIKQ_EXTERNAL_WAIT_SECONDS"] = wait_seconds
    import main as refund_main

    refund_main.STORE = demo_store(fault)
    if fault == Fault.DELAYED_SETTLEMENT:
        refund_main.STORE.settlement_delay_seconds = 2.0
    agent = build_agent()
    goal = build_goal()
    async with Runtime(agent, endpoint=endpoint, state_provider=StoreProvider()) as rt:
        return await rt.run(goal, input={"order_id": "ORD-2048", "fault": fault}), refund_main.STORE


async def crash_and_resume(barrier: str, fault: str = Fault.DELAYED_SETTLEMENT):
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port, crash_at=barrier)
    try:
        await wait_ready(url)
        task = asyncio.create_task(refund_run(url, fault))
        code = await wait_exit(proc, timeout=90)
        assert code == 97, f"expected crash exit 97 at {barrier}, got {code}"
        proc = start_server(port, crash_at=None)
        await wait_ready(url, timeout=45)
        result, store = await asyncio.wait_for(task, timeout=90)
        return result, store
    finally:
        if proc.poll() is None:
            proc.kill()


@pytest.mark.usefixtures("require_env")
@pytest.mark.parametrize(
    "barrier",
    ["before_dispatch", "after_intent", "during_tool", "after_accept", "during_verify"],
)
def test_refund_crash_boundary(barrier: str) -> None:
    result, store = asyncio.run(crash_and_resume(barrier))
    refunds = [r for r in store.refunds.values() if r.order_id == "ORD-2048" and r.status != "rejected"]
    assert len(refunds) <= 1, f"duplicate refunds after {barrier}: {refunds}"
    assert result.status in {"COMPLETED", "UNVERIFIED", "RECOVERY_REQUIRED"}
    if result.status == "COMPLETED":
        assert result.verified
        assert store.orders["ORD-2048"].status == "refunded"


@pytest.mark.usefixtures("require_env")
def test_approval_survives_restart() -> None:
    asyncio.run(_approval_survives_restart())


async def _approval_survives_restart() -> None:
    from goshikq_ai import tool
    from goshikq_ai.state import SnapshotContext, StateSnapshot

    claims = {"CLM-77": {"id": "CLM-77", "coverage": True, "reserve_cents": 15000, "status": "open", "payout_cents": 0}}

    @tool(effect="read_only")
    def lookup_claim(claim_id: str) -> dict:
        return claims.get(claim_id, {"found": False})

    @tool(effect="non_idempotent_write", risk="critical", requires_approval=True, permissions=["claims:payout"])
    def authorize_payout(claim_id: str, amount_cents: int) -> dict:
        claims[claim_id]["status"] = "paid"
        claims[claim_id]["payout_cents"] = amount_cents
        return {"paid": True, "claim_id": claim_id, "amount_cents": amount_cents}

    class Provider:
        def snapshot(self, context: SnapshotContext) -> StateSnapshot:
            return StateSnapshot(state={"claims": claims}, version=str(claims["CLM-77"]["payout_cents"]), source="claims-memory")

    port = free_port()
    url = f"http://127.0.0.1:{port}"
    # Do not set GOSHIKQ_CRASH_AT=waiting_approval on a shared database: the
    # supervisor would resume leftover WAITING_FOR_APPROVAL runs and abort
    # before this test's create_run. Kill after *this* approval is pending.
    proc = start_server(port, crash_at=None)
    agent_name = f"crash_claims_agent_{int(time.time())}"
    agent = Agent(
        name=agent_name,
        model=MockModel(
            script=[
                ModelTurn(tool_calls=[("lookup_claim", {"claim_id": "CLM-77"})]),
                ModelTurn(tool_calls=[("authorize_payout", {"claim_id": "CLM-77", "amount_cents": 15000})]),
                ModelTurn(content="Payout authorized."),
            ]
        ),
        tools=[lookup_claim, authorize_payout],
        permissions=["claims:payout"],
    )
    goal = Goal(description="Pay covered claim CLM-77", success=lambda s: (s.get("claims") or {}).get("CLM-77", {}).get("status") == "paid", max_steps=8)
    try:
        await wait_ready(url)

        async def serve():
            async with Runtime(agent, endpoint=url, state_provider=Provider()) as rt:
                return await rt.run(goal, input={"claim_id": "CLM-77"})

        task = asyncio.create_task(serve())
        run_id = None
        async with httpx.AsyncClient() as client:
            deadline = time.monotonic() + 45
            while time.monotonic() < deadline:
                listed = await client.get(f"{url}/api/v1/runs", params={"limit": 30})
                items = listed.json().get("items") or [] if listed.status_code == 200 else []
                mine = [i for i in items if i.get("agent_name") == agent_name]
                if mine:
                    run_id = mine[0]["id"]
                    if mine[0].get("status") == "WAITING_FOR_APPROVAL":
                        break
                await asyncio.sleep(0.3)
            else:
                raise AssertionError(f"{agent_name} did not reach WAITING_FOR_APPROVAL")
        if proc.poll() is None:
            proc.kill()
            proc.wait(timeout=15)
        proc = start_server(port, crash_at=None)
        await wait_ready(url, timeout=45)
        async with httpx.AsyncClient() as client:
            deadline = time.monotonic() + 30
            approval_id = None
            while time.monotonic() < deadline:
                r = await client.get(f"{url}/api/v1/approvals", params={"status": "pending", "run_id": run_id})
                if r.status_code == 200:
                    items = r.json() if isinstance(r.json(), list) else r.json().get("items") or r.json()
                    if isinstance(items, dict):
                        items = items.get("items") or []
                    items = [i for i in items if i.get("run_id") == run_id] or items
                    if items:
                        approval_id = items[0]["id"]
                        rec = await client.get(f"{url}/api/v1/runs/{run_id}/recovery")
                        assert rec.status_code == 200
                        break
                await asyncio.sleep(0.4)
            assert approval_id, "approval did not survive restart"
            dec = await client.post(
                f"{url}/api/v1/approvals/{approval_id}/decision",
                json={"decision": "approve", "reason": "crash-test"},
                headers={"x-goshikq-actor": "crash-test"},
            )
            assert dec.status_code < 300, dec.text
        result = await asyncio.wait_for(task, timeout=90)
        assert claims["CLM-77"]["status"] == "paid"
        assert claims["CLM-77"]["payout_cents"] == 15000
        assert result.status in {"COMPLETED", "UNVERIFIED"}
    finally:
        if proc.poll() is None:
            proc.kill()


async def wait_run_id(url: str, timeout: float = 30.0, agent: str | None = "refund_agent") -> str:
    deadline = time.monotonic() + timeout
    async with httpx.AsyncClient() as client:
        while time.monotonic() < deadline:
            r = await client.get(f"{url}/api/v1/runs", params={"limit": 20})
            if r.status_code == 200:
                items = r.json().get("items") or []
                if agent:
                    items = [i for i in items if i.get("agent_name") == agent]
                if items:
                    return items[0]["id"]
            await asyncio.sleep(0.3)
    raise TimeoutError("no run appeared after restart")


async def collect_sse_sequences(url: str, run_id: str, after: int, timeout: float = 60.0) -> list[int]:
    seqs: list[int] = []
    async with httpx.AsyncClient(timeout=timeout) as client:
        async with client.stream(
            "GET",
            f"{url}/api/v1/runs/{run_id}/events/stream",
            params={"after": after},
            headers={"Last-Event-ID": str(after)},
        ) as resp:
            async for line in resp.aiter_lines():
                if line.startswith("id:"):
                    seqs.append(int(line.split(":", 1)[1].strip()))
                if line.startswith("event: end"):
                    break
    return seqs


@pytest.mark.usefixtures("require_env")
def test_sse_reconnect_after_crash() -> None:
    asyncio.run(_sse_reconnect_after_crash())


async def _sse_reconnect_after_crash() -> None:
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port, crash_at="after_accept")
    try:
        await wait_ready(url)
        task = asyncio.create_task(refund_run(url))
        code = await wait_exit(proc, timeout=90)
        assert code == 97
        proc = start_server(port, crash_at=None)
        await wait_ready(url, timeout=45)
        run_id = await wait_run_id(url)
        async with httpx.AsyncClient(timeout=30) as client:
            hist = await client.get(f"{url}/api/v1/runs/{run_id}/events", params={"limit": 500})
            hist.raise_for_status()
            items = hist.json()["items"]
            hist_seqs = [int(e["sequence"]) for e in items]
            assert hist_seqs == sorted(hist_seqs)
            assert len(hist_seqs) == len(set(hist_seqs))
            after = hist_seqs[-1] if hist_seqs else 0
            rec = await client.get(f"{url}/api/v1/runs/{run_id}/recovery")
            assert rec.status_code == 200
        live = await collect_sse_sequences(url, run_id, after)
        for n in live:
            assert n not in hist_seqs
        combined = hist_seqs + live
        assert combined == sorted(combined)
        assert combined == list(range(1, (combined[-1] if combined else 0) + 1))
        result, store = await asyncio.wait_for(task, timeout=90)
        refunds = [r for r in store.refunds.values() if r.order_id == "ORD-2048" and r.status != "rejected"]
        assert len(refunds) <= 1
        assert result.status in {"COMPLETED", "UNVERIFIED", "RECOVERY_REQUIRED"}
    finally:
        if proc.poll() is None:
            proc.kill()


@pytest.mark.usefixtures("require_env")
def test_never_settles_inconclusive() -> None:
    asyncio.run(_never_settles_inconclusive())


async def _never_settles_inconclusive() -> None:
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port, crash_at=None)
    try:
        await wait_ready(url)
        result, store = await refund_run(url, Fault.NEVER_SETTLES, wait_seconds="4")
        assert result.status == "UNVERIFIED"
        assert result.verification_status == "INCONCLUSIVE"
        assert result.status not in {"STEP_LIMIT_EXCEEDED", "MODEL_BUDGET_EXCEEDED", "BUDGET_EXCEEDED"}
        reason = result.failure_reason or ""
        assert "INCONCLUSIVE" in reason or "wait deadline" in reason.lower() or "settlement" in reason.lower()
        refunds = [r for r in store.refunds.values() if r.order_id == "ORD-2048" and r.status != "rejected"]
        assert len(refunds) == 1
        assert refunds[0].status != "settled"
    finally:
        if proc.poll() is None:
            proc.kill()
