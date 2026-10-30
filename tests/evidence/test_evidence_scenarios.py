"""Live evidence-change scenarios (Axiom v0.3).

Requires DATABASE_URL and an axiom-server binary, same as tests/crash.
"""

from __future__ import annotations

import asyncio
import os
import sys
import time
from pathlib import Path

import httpx
import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "examples" / "retail-refund"))
sys.path.insert(0, str(ROOT / "examples" / "insurance-claims"))
sys.path.insert(0, str(ROOT / "sdk" / "python" / "src"))
sys.path.insert(0, str(ROOT / "tests" / "crash"))

from store import Fault  # noqa: E402
from test_crash_boundaries import find_server, free_port, start_server, wait_ready  # noqa: E402

DATABASE_URL = os.environ.get("DATABASE_URL", "postgres://postgres:axiom@127.0.0.1:55432/axiom")


@pytest.fixture
def require_env() -> None:
    if find_server() is None:
        pytest.skip("axiom-server binary not found")


async def wait_pending_approval(url: str, timeout: float = 30.0) -> dict:
    deadline = time.monotonic() + timeout
    async with httpx.AsyncClient(timeout=10) as client:
        while time.monotonic() < deadline:
            r = await client.get(f"{url}/api/v1/approvals?status=pending")
            if r.status_code == 200:
                payload = r.json()
                items = payload.get("items") if isinstance(payload, dict) else payload
                items = [i for i in (items or []) if i.get("agent_name") == "claims_agent"]
                if items:
                    return items[0]
            await asyncio.sleep(0.3)
    raise TimeoutError("no pending approval")


@pytest.mark.usefixtures("require_env")
def test_insurance_policy_change_invalidates_approval() -> None:
    asyncio.run(_insurance_policy_change())


async def _insurance_policy_change() -> None:
    import importlib.util

    spec = importlib.util.spec_from_file_location("insurance_claims_main", ROOT / "examples" / "insurance-claims" / "main.py")
    claims = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(claims)

    claims.reset_world()
    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port)
    try:
        await wait_ready(url)

        os.environ["AXIOM_RUNTIME_URL"] = url
        task = asyncio.create_task(claims.run_once(mutate=False))
        approval = await wait_pending_approval(url)
        run_id = approval["run_id"]
        async with httpx.AsyncClient(timeout=20) as client:
            ev = await client.get(f"{url}/api/v1/runs/{run_id}/evidence")
            ev.raise_for_status()
            body = ev.json()
            assert body["records"], "coverage evidence should persist"
            versions = {r["resource_id"]: r["resource_version"] for r in body["records"]}
            assert versions.get("policy:POL-1") == "4"
            assert any(d["id"] == "dec:coverage:CLM-77" for d in body["decisions"])

            claims.mutate_policy_to_v5()
            dec = await client.post(
                f"{url}/api/v1/approvals/{approval['id']}/decision",
                json={"decision": "approve", "reason": "stale-policy-test"},
                headers={"x-axiom-actor": "evidence-test"},
            )
            assert dec.status_code < 300, dec.text
        code = await asyncio.wait_for(task, timeout=90)
        assert claims.CLAIMS["CLM-77"]["payout_cents"] == 0
        assert claims.CLAIMS["CLM-77"]["status"] != "paid"
        assert code == 1
        async with httpx.AsyncClient(timeout=20) as client:
            ev = await client.get(f"{url}/api/v1/runs/{run_id}/evidence")
            ev.raise_for_status()
            body = ev.json()
            assert body["impact"], "change impact should be recorded"
            assert body["revalidation"], "revalidation history should be recorded"
            events = await client.get(f"{url}/api/v1/runs/{run_id}/events", params={"limit": 500})
            events.raise_for_status()
            types = [e["event_type"] for e in events.json()["items"]]
            assert "approval.granted" in types
            assert "approval.invalidated" in types
            granted_seq = next(e["sequence"] for e in events.json()["items"] if e["event_type"] == "approval.granted")
            invalidated_seq = next(e["sequence"] for e in events.json()["items"] if e["event_type"] == "approval.invalidated")
            assert invalidated_seq > granted_seq
            hist = events.json()["items"]
            granted = [e for e in hist if e["event_type"] == "approval.granted"]
            assert granted[0]["event_type"] == "approval.granted"
            rec = await client.get(f"{url}/api/v1/runs/{run_id}/recovery")
            rec.raise_for_status()
            run = await client.get(f"{url}/api/v1/runs/{run_id}")
            run.raise_for_status()
            assert run.json()["status"] == "FAILED"
            assert rec.json().get("recovery_disposition") == "REVALIDATION_REQUIRED"
    finally:
        if proc.poll() is None:
            proc.kill()
        os.environ.pop("AXIOM_RUNTIME_URL", None)


@pytest.mark.usefixtures("require_env")
def test_retail_return_change_blocks_refund() -> None:
    asyncio.run(_retail_return_change())


async def _retail_return_change() -> None:
    import main as refund_main

    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port)
    try:
        await wait_ready(url)
        os.environ["AXIOM_RUNTIME_URL"] = url
        code = await refund_main.run_once("ORD-2048", Fault.RETURN_CHANGED)
        assert refund_main.STORE.submit_count == 0
        assert refund_main.STORE.orders["ORD-2048"].status != "refunded"
        assert code != 0
    finally:
        if proc.poll() is None:
            proc.kill()
        os.environ.pop("AXIOM_RUNTIME_URL", None)


@pytest.mark.usefixtures("require_env")
def test_never_settles_keeps_acknowledged_dispatch() -> None:
    asyncio.run(_never_settles_dispatch())


async def _never_settles_dispatch() -> None:
    import main as refund_main
    from store import Fault as F

    port = free_port()
    url = f"http://127.0.0.1:{port}"
    proc = start_server(port)
    try:
        await wait_ready(url)
        os.environ["AXIOM_RUNTIME_URL"] = url
        os.environ["AXIOM_EXTERNAL_WAIT_SECONDS"] = "3"
        await refund_main.run_once("ORD-2048", F.NEVER_SETTLES)
        async with httpx.AsyncClient(timeout=20) as client:
            runs = await client.get(f"{url}/api/v1/runs", params={"limit": 5})
            runs.raise_for_status()
            items = [i for i in runs.json()["items"] if i.get("agent_name") == "refund_agent"]
            assert items
            run_id = items[0]["id"]
            rec = await client.get(f"{url}/api/v1/runs/{run_id}/recovery")
            rec.raise_for_status()
            body = rec.json()
            assert body["dispatch_state"] in {"ACCEPTED", "OUTCOME_PENDING"}
            assert body["dispatch_state"] != "DISPATCH_UNCERTAIN"
            assert body["dispatch_state"] != "RECOVERY_REQUIRED"
            assert body["business_operation_status"] == "PENDING_SETTLEMENT"
            assert body["verification_status"] == "INCONCLUSIVE"
            assert body["recovery_disposition"] == "MANUAL_RECONCILIATION_REQUIRED"
            assert body.get("uncertain_write") is not True
            warning = body.get("warning") or ""
            assert "not uncertain" in warning.lower() or "acknowledged" in warning.lower()
            events = await client.get(f"{url}/api/v1/runs/{run_id}/events", params={"limit": 500})
            seqs = [e["sequence"] for e in events.json()["items"]]
            assert seqs == sorted(seqs)
            assert len(seqs) == len(set(seqs))
            journal_types = [e["event_type"] for e in events.json()["items"]]
            assert "tool.completed" in journal_types
    finally:
        if proc.poll() is None:
            proc.kill()
        os.environ.pop("AXIOM_RUNTIME_URL", None)
        os.environ.pop("AXIOM_EXTERNAL_WAIT_SECONDS", None)
