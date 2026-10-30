"""Retail refund demo: simulated orders + payment processor, verified against store state.

A processor `accepted` response does not complete the goal. The success predicate reads
the order row and the refund settlement status from SimulatedStore.

Usage:
    python main.py                         # happy path (ORD-2048)
    python main.py --fault processor_reject
    python main.py --fault delayed_settlement
    python main.py --fault never_settles
    python main.py --order ORD-LOCKED      # ineligible; goal fails
    python main.py --fault return_changed  # return status flips after eligibility; refund blocked
"""

from __future__ import annotations

import argparse
import asyncio
import logging
import os
import sys

from axiom_ai import Agent, DecisionRef, Evidence, Goal, InvariantOutcome, Runtime, StateSnapshot, tool, with_evidence
from axiom_ai.models import MockModel, ModelTurn, last_tool_result
from axiom_ai.state import SnapshotContext

from store import Fault, SimulatedStore, demo_store

STORE: SimulatedStore = demo_store()


def _verify_refund(arguments: dict, output, state: StateSnapshot):
    from axiom_ai.tools import VerifierResult

    order_id = arguments.get("order_id")
    order = (state.get("orders") or {}).get(order_id or "")
    if not order:
        return VerifierResult.inconclusive("order missing from snapshot")
    refunds = state.get("refunds") or {}
    refund = refunds.get(order.get("refund_id") or "")
    refund_status = (refund or {}).get("status")
    actual = {"order.status": order.get("status"), "refund.status": refund_status, "business_operation_status": refund_status}
    if order.get("status") == "refunded" and refund_status == "settled":
        return VerifierResult.verified(expected="settled", actual=actual, method="order.status+refund.status")
    if output and isinstance(output, dict) and output.get("accepted") and refund_status != "settled":
        return VerifierResult.waiting(
            "processor accepted the request but settlement is pending; do not re-dispatch",
            expected={"order.status": "refunded", "refund.status": "settled"},
            actual=actual,
            method="order.status+refund.status",
            business_operation_status="PENDING_SETTLEMENT",
        )
    if refund_status == "rejected":
        return VerifierResult.failed("processor rejected the refund", expected="settled", actual=actual, method="refund.status")
    return VerifierResult.inconclusive(f"order status is {order.get('status')}", method="order.status")


@tool(effect="read_only")
def lookup_order(order_id: str) -> dict:
    """Read the simulated order database."""
    return STORE.lookup_order(order_id)


@tool(effect="read_only")
def check_eligibility(order_id: str) -> dict:
    """Deterministic return-window / policy check."""
    raw = STORE.check_eligibility(order_id)
    order = STORE.orders.get(order_id)
    version = str(order.version) if order else None
    return with_evidence(
        raw,
        [
            Evidence(
                source_system="orders-db",
                resource_id=f"order:{order_id}",
                resource_version=version,
                trust="authoritative",
                access_scope="refunds-read",
            )
        ],
        DecisionRef(
            id=f"dec:eligibility:{order_id}",
            kind="eligibility",
            depends_on=[f"order:{order_id}"],
            action="act:submit_refund",
            payload={"resource_id": f"order:{order_id}"},
        ),
    )


@tool(
    effect="idempotent_write",
    idempotent=True,
    risk="high",
    permissions=["payments:refund"],
    verifier=_verify_refund,
    required_evidence=["order:{order_id}"],
    required_invariants=["eligibility"],
    on_missing_evidence="block",
    revalidation_policy="strict_contract",
)
def submit_refund(order_id: str, amount_cents: int, idempotency_key: str | None = None) -> dict:
    """Submit a refund to the simulated processor. Honors idempotency_key."""
    key = idempotency_key or f"refund:{order_id}:{amount_cents}"
    return STORE.submit_refund(order_id, amount_cents, key)


@tool(effect="read_only")
def get_refund_status(refund_id: str) -> dict:
    """Poll the processor for settlement. Required after an `accepted` (not settled) response."""
    return STORE.get_refund(refund_id)


class StoreProvider:
    def snapshot(self, context: SnapshotContext) -> StateSnapshot:
        if STORE.fault == Fault.MISSING_EVIDENCE and context.reason in ("final", "verify"):
            raise RuntimeError("state provider unavailable (injected fault missing_evidence)")
        snap = STORE.snapshot()
        versions = {oid: o["version"] for oid, o in snap["orders"].items()}
        return StateSnapshot(state=snap, version=str(versions), source="simulated-orders+processor")

    def version(self, resource_id: str) -> str | None:
        order = STORE.orders.get(resource_id)
        return str(order.version) if order else None


def refund_completed(state: StateSnapshot) -> InvariantOutcome:
    orders = state.get("orders") or {}
    order_id = os.environ.get("AXIOM_REFUND_ORDER", "ORD-2048")
    target = orders.get(order_id)
    if not target:
        return InvariantOutcome(False, reason=f"order {order_id} missing", expected="present", actual=None)
    refunds = state.get("refunds") or {}
    refund = refunds.get(target.get("refund_id") or "")
    settled = target.get("status") == "refunded" and (refund or {}).get("status") == "settled"
    return InvariantOutcome(
        settled,
        expected={"order.status": "refunded", "refund.status": "settled"},
        actual={"order.status": target.get("status"), "refund.status": (refund or {}).get("status")},
        reason=None if settled else "refund has not settled against authoritative state",
    )


def refund_not_duplicated(state: StateSnapshot) -> InvariantOutcome:
    order_id = os.environ.get("AXIOM_REFUND_ORDER", "ORD-2048")
    matches = [r for r in (state.get("refunds") or {}).values() if r.get("order_id") == order_id and r.get("status") != "rejected"]
    ok = len(matches) <= 1
    return InvariantOutcome(ok, expected={"max_refunds": 1}, actual=len(matches), reason=None if ok else "duplicate refund records")


def amount_within_authorized_limit(state: StateSnapshot) -> InvariantOutcome:
    limit = state.get("authorized_limit_cents") or 0
    for r in (state.get("refunds") or {}).values():
        if r.get("amount_cents", 0) > limit:
            return InvariantOutcome(False, expected={"max": limit}, actual=r.get("amount_cents"), reason="amount exceeds authorized limit")
    return InvariantOutcome(True, expected={"max": limit}, actual="ok")


def refund_policy(messages, tools, idx: int) -> ModelTurn:
    order_id = os.environ.get("AXIOM_REFUND_ORDER", "ORD-2048")
    if idx == 0:
        return ModelTurn(tool_calls=[("lookup_order", {"order_id": order_id})])
    if idx == 1:
        return ModelTurn(tool_calls=[("check_eligibility", {"order_id": order_id})])
    elig = last_tool_result(messages, "check_eligibility") or {}
    inner = elig.get("result", elig)
    if not inner.get("eligible"):
        return ModelTurn(content=f"Cannot refund {order_id}: {inner.get('reason')}")
    if idx == 2:
        amount = inner.get("amount_cents", 0)
        return ModelTurn(tool_calls=[("submit_refund", {"order_id": order_id, "amount_cents": amount})])
    submitted = last_tool_result(messages, "submit_refund") or {}
    payload = submitted.get("result", submitted)
    refund = payload.get("refund") or {}
    if payload.get("accepted") and not payload.get("settled") and refund.get("id"):
        return ModelTurn(tool_calls=[("get_refund_status", {"refund_id": refund["id"]})])
    if refund.get("status") == "settled" or payload.get("settled"):
        return ModelTurn(content=f"Refund {refund.get('id')} settled for {order_id}.")
    return ModelTurn(content=f"Refund did not settle: {payload}")


def build_agent() -> Agent:
    return Agent(
        name="refund_agent",
        model=MockModel(policy=refund_policy),
        instructions="Resolve valid customer refunds. Never claim success without tool evidence.",
        tools=[lookup_order, check_eligibility, submit_refund, get_refund_status],
        permissions=["payments:refund"],
    )


def build_goal() -> Goal:
    return Goal(
        description=f"Resolve refund for {os.environ.get('AXIOM_REFUND_ORDER', 'ORD-2048')}",
        success=refund_completed,
        invariants=[refund_not_duplicated, amount_within_authorized_limit],
        max_steps=10,
        external_wait_seconds=int(os.environ.get("AXIOM_EXTERNAL_WAIT_SECONDS", "8" if os.environ.get("AXIOM_REFUND_FAULT") == Fault.NEVER_SETTLES else "20")),
    )


async def run_once(order_id: str, fault: str) -> int:
    global STORE
    os.environ["AXIOM_REFUND_ORDER"] = order_id
    os.environ["AXIOM_REFUND_FAULT"] = fault
    STORE = demo_store(fault)
    agent = build_agent()
    goal = build_goal()
    async with Runtime(agent, state_provider=StoreProvider()) as rt:
        result = await rt.run(goal, input={"order_id": order_id, "fault": fault})
    print(result.summary())
    print(f"store_order_status: {STORE.orders[order_id].status}")
    if STORE.orders[order_id].refund_id:
        print(f"store_refund_status: {STORE.refunds[STORE.orders[order_id].refund_id].status}")
    return 0 if result.verified else 1


def main() -> int:
    logging.basicConfig(level=os.environ.get("AXIOM_LOG", "WARNING"))
    p = argparse.ArgumentParser(description="Axiom retail refund demo")
    p.add_argument("--order", default="ORD-2048")
    p.add_argument("--fault", default=Fault.NONE, choices=[Fault.NONE, Fault.PROCESSOR_REJECT, Fault.DELAYED_SETTLEMENT, Fault.NEVER_SETTLES, Fault.STALE_ORDER, Fault.MISSING_EVIDENCE, Fault.RETURN_CHANGED])
    args = p.parse_args()
    return asyncio.run(run_once(args.order, args.fault))


if __name__ == "__main__":
    sys.exit(main())
