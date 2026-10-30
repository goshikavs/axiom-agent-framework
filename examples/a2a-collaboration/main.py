"""Example C — two independent processes talking A2A 1.0 JSON-RPC.

Process 1: standalone A2A peer (this script's subprocess).
Process 2: this client, which also records an Axiom run for the parent goal.

Remote completed/SUCCESS is not parent VERIFIED. The parent goal still reads
local simulated state.
"""

from __future__ import annotations

import asyncio
import os
import socket
import subprocess
import sys
import time
from pathlib import Path

from axiom_ai import A2aClient, Agent, Goal, Runtime, StateSnapshot
from axiom_ai.models import MockModel, ModelTurn
from axiom_ai.state import SnapshotContext

ROOT = Path(__file__).resolve().parent
STATE = {"remote_ok": False, "remote_task_id": None, "remote_state": None}


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


class Provider:
    def snapshot(self, context: SnapshotContext) -> StateSnapshot:
        return StateSnapshot(state=dict(STATE), version="1", source="local-supervisor")


def parent_ok(snap: StateSnapshot) -> bool:
    # Remote SUCCESS is not enough; this predicate requires local recording plus a completed peer.
    return bool(snap.get("remote_ok")) and snap.get("remote_state") == "completed"


async def run_scenario(text: str, expect_verified: bool) -> str:
    port = free_port()
    proc = subprocess.Popen([sys.executable, str(ROOT / "peer.py"), str(port)], cwd=str(ROOT))
    try:
        card_ok = False
        deadline = time.monotonic() + 10
        peer = A2aClient(f"http://127.0.0.1:{port}")
        while time.monotonic() < deadline:
            try:
                card = await peer.agent_card()
                assert card.get("protocolVersion") == "1.0" or any(
                    i.get("protocolVersion") == "1.0" for i in card.get("supportedInterfaces") or []
                )
                card_ok = True
                break
            except Exception:
                time.sleep(0.1)
        if not card_ok:
            raise SystemExit("peer did not serve an Agent Card")
        task = await peer.send_message(text)
        task_id = str(task["id"])
        got = await peer.get_task(task_id)
        remote_state = (got.get("status") or {}).get("state")
        STATE["remote_task_id"] = task_id
        STATE["remote_state"] = remote_state
        # Explicitly do not treat remote SUCCESS as verification:
        STATE["remote_ok"] = remote_state == "completed" and "FAIL" not in text.upper()
        agent = Agent(name="a2a-supervisor", model=MockModel(script=[ModelTurn(content="record remote assertion")]))
        result = await Runtime(agent, state_provider=Provider(), endpoint=os.environ.get("AXIOM_RUNTIME_URL")).run(
            Goal(description="Local supervisor independently confirms the collaboration outcome", success=parent_ok),
            input={"a2a_task_id": task_id, "a2a_state": remote_state, "untrusted": True},
            metadata={"example": "a2a-collaboration", "a2a_task_id": task_id},
        )
        print(f"scenario={text!r} remote_state={remote_state} parent={result.status}/{result.verification_status} run={result.run_id}")
        print(f"remote_completed_is_not_verified={remote_state == 'completed' and result.verification_status != 'VERIFIED' or result.verified == (remote_state == 'completed' and expect_verified)}")
        if expect_verified and not result.verified:
            raise SystemExit("expected parent VERIFIED")
        if not expect_verified and result.verified:
            raise SystemExit("parent must not verify a failed remote task")
        return result.run_id
    finally:
        proc.kill()
        proc.wait(timeout=5)
        await peer.aclose()


async def main() -> int:
    ok_id = await run_scenario("collaborate on inventory lookup", True)
    fail_id = await run_scenario("FAIL please", False)
    print(f"ok_run={ok_id}")
    print(f"fail_run={fail_id}")
    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
