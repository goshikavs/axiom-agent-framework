"""Example A — multi-agent retail refund with verified parent outcome.

Child agents may complete successfully. The supervisor still evaluates the
refund against the simulated store. Child COMPLETED is not parent VERIFIED.
"""

from __future__ import annotations

import asyncio
import os
import sys

from goshikq_ai import Agent, AgentGroup, AgentTask, CoordinationPolicy, Goal, StateSnapshot, tool
from goshikq_ai.models import MockModel, ModelTurn
from goshikq_ai.state import SnapshotContext

from store import STORE

STORE.reset()


@tool(effect="read_only")
def check_eligibility(order_id: str) -> dict:
    order = STORE.orders.get(order_id) or {}
    ok = bool(order.get("eligible") and order.get("status") == "paid")
    STORE.orders[order_id] = {**order, "eligibility": ok}
    return {"order_id": order_id, "eligible": ok, "version": order.get("version")}


@tool(effect="read_only")
def inspect_return(order_id: str) -> dict:
    STORE.inspections[order_id] = {"order_id": order_id, "passed": True, "condition": "resaleable"}
    return STORE.inspections[order_id]


@tool(effect="idempotent_write", idempotent=True)
def record_decision(order_id: str) -> dict:
    eligible = bool((STORE.orders.get(order_id) or {}).get("eligibility"))
    passed = bool((STORE.inspections.get(order_id) or {}).get("passed"))
    approved = eligible and passed
    STORE.decisions[order_id] = {"order_id": order_id, "approved": approved}
    return STORE.decisions[order_id]


@tool(effect="idempotent_write", idempotent=True)
def settle_refund(order_id: str, amount_cents: int, idempotency_key: str | None = None) -> dict:
    key = idempotency_key or f"refund:{order_id}"
    if key in {r.get("key") for r in STORE.refunds.values()}:
        return next(r for r in STORE.refunds.values() if r.get("key") == key)
    if not (STORE.decisions.get(order_id) or {}).get("approved"):
        return {"accepted": False, "reason": "decision_not_approved"}
    rid = f"rf-{order_id}"
    STORE.refunds[rid] = {"id": rid, "order_id": order_id, "amount_cents": amount_cents, "status": "settled", "key": key}
    STORE.orders[order_id]["status"] = "refunded"
    STORE.orders[order_id]["refund_id"] = rid
    return STORE.refunds[rid]


def refund_settled(snap: StateSnapshot) -> bool:
    order = (snap.get("orders") or {}).get("ORD-2048") or {}
    refund = (snap.get("refunds") or {}).get(order.get("refund_id") or "") or {}
    return order.get("status") == "refunded" and refund.get("status") == "settled"


def child_true(_snap: StateSnapshot) -> bool:
    return True


class StoreProvider:
    def snapshot(self, context: SnapshotContext) -> StateSnapshot:
        snap = STORE.snapshot()
        return StateSnapshot(state=snap, version=str((snap["orders"].get("ORD-2048") or {}).get("version")), source="team-store")


def _agent(name: str, tools: list, turns: list[ModelTurn]) -> Agent:
    return Agent(name=name, model=MockModel(script=turns), tools=tools, instructions=name)


async def main() -> int:
    provider = StoreProvider()
    eligibility = _agent(
        "order-eligibility",
        [check_eligibility],
        [ModelTurn(tool_calls=[("check_eligibility", {"order_id": "ORD-2048"})]), ModelTurn(content="eligible")],
    )
    inspection = _agent(
        "return-inspection",
        [inspect_return],
        [ModelTurn(tool_calls=[("inspect_return", {"order_id": "ORD-2048"})]), ModelTurn(content="passed")],
    )
    decision = _agent(
        "refund-decision",
        [record_decision],
        [ModelTurn(tool_calls=[("record_decision", {"order_id": "ORD-2048"})]), ModelTurn(content="approved")],
    )
    settlement = _agent(
        "settlement-verification",
        [settle_refund],
        [ModelTurn(tool_calls=[("settle_refund", {"order_id": "ORD-2048", "amount_cents": 4200})]), ModelTurn(content="settled")],
    )
    supervisor = Agent(
        name="refund-supervisor",
        model=MockModel(script=[ModelTurn(content="children finished; verify store")]),
        instructions="Do not trust child reports. The runtime verifies the store.",
    )
    group = AgentGroup(
        supervisor,
        policy=CoordinationPolicy(mode="sequential", max_depth=4, max_delegations=8),
        state_provider=provider,
        endpoint=os.environ.get("GOSHIKQ_RUNTIME_URL"),
    )
    child_goal = Goal(description="Complete assigned refund subtask", success=child_true)
    result = await group.run(
        Goal(description="Refund ORD-2048 is settled in the store", success=refund_settled),
        [
            AgentTask(eligibility, child_goal, input={"order_id": "ORD-2048"}, role="eligibility"),
            AgentTask(inspection, child_goal, input={"order_id": "ORD-2048"}, role="inspection"),
            AgentTask(decision, child_goal, input={"order_id": "ORD-2048"}, role="decision"),
            AgentTask(settlement, child_goal, input={"order_id": "ORD-2048"}, role="settlement"),
        ],
        input={"order_id": "ORD-2048"},
        metadata={"example": "multi-agent-refund"},
    )
    print(result.parent.summary())
    print("children:")
    for c in result.children:
        print(f"  {c.run_id} {c.status} verify={c.verification_status}")
    print(f"parent_verified={result.verified}")
    # Child success must not be confused with parent verification.
    if any(c.status in {"COMPLETED", "UNVERIFIED"} for c in result.children) and not result.verified:
        print("note: at least one child finished without parent VERIFIED")
    return 0 if result.verified else 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
