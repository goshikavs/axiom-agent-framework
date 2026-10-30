"""Insurance claim demo with a human approval gate and evidence-aware revalidation.

The payout tool requires approval. The run pauses at WAITING_FOR_APPROVAL until
Studio (or an API client) posts a decision to /api/v1/approvals/{id}/decision.

`--mutate-policy` flips POL-1 from version 4 to version 5 while approval is pending.
The prior coverage decision is marked for revalidation and the old approval cannot
authorize the payout.
"""

from __future__ import annotations

import argparse
import asyncio
import logging
import os
import sys

from axiom_ai import Agent, DecisionRef, Evidence, Goal, InvariantOutcome, Runtime, StateSnapshot, tool, with_evidence
from axiom_ai.evidence import resource_hint
from axiom_ai.models import MockModel, ModelTurn
from axiom_ai.state import SnapshotContext

POLICIES = {
    "POL-1": {"id": "POL-1", "version": 4, "coverage": True, "reserve_cents": 15000},
}
CLAIMS = {
    "CLM-77": {"id": "CLM-77", "policy_id": "POL-1", "coverage": True, "status": "open", "payout_cents": 0, "recommended_cents": 15000},
}


def reset_world() -> None:
    POLICIES["POL-1"] = {"id": "POL-1", "version": 4, "coverage": True, "reserve_cents": 15000}
    CLAIMS["CLM-77"] = {"id": "CLM-77", "policy_id": "POL-1", "coverage": True, "status": "open", "payout_cents": 0, "recommended_cents": 15000}


def mutate_policy_to_v5() -> None:
    POLICIES["POL-1"] = {"id": "POL-1", "version": 5, "coverage": True, "reserve_cents": 5000}


@tool(effect="read_only")
def lookup_claim(claim_id: str) -> dict:
    return CLAIMS.get(claim_id, {"found": False})


@tool(effect="read_only")
def check_coverage(claim_id: str) -> dict:
    c = CLAIMS.get(claim_id)
    if not c:
        return {"covered": False, "reason": "unknown"}
    p = POLICIES.get(c.get("policy_id") or "POL-1") or {}
    covered = bool(c.get("coverage") and p.get("coverage"))
    body = {"covered": covered, "reserve_cents": p.get("reserve_cents"), "policy_version": p.get("version")}
    return with_evidence(
        body,
        [
            Evidence(
                source_system="policy-admin",
                resource_id="policy:POL-1",
                resource_version=str(p.get("version")),
                trust="authoritative",
                access_scope="claims-read",
            )
        ],
        DecisionRef(
            id="dec:coverage:CLM-77",
            kind="coverage",
            depends_on=["policy:POL-1"],
            action="act:authorize_payout",
            payload={"resource_id": "policy:POL-1", "amount_cents": c.get("recommended_cents", 15000)},
        ),
    )


@tool(
    effect="non_idempotent_write",
    risk="critical",
    requires_approval=True,
    permissions=["claims:payout"],
    required_evidence=["policy:POL-1"],
    required_invariants=["coverage"],
    on_missing_evidence="block",
    revalidation_policy="strict_contract",
)
def authorize_payout(claim_id: str, amount_cents: int) -> dict:
    c = CLAIMS[claim_id]
    c["status"] = "paid"
    c["payout_cents"] = amount_cents
    return {"paid": True, "claim_id": claim_id, "amount_cents": amount_cents}


class ClaimsState:
    def snapshot(self, context: SnapshotContext) -> StateSnapshot:
        state = {
            "claims": {k: dict(v) for k, v in CLAIMS.items()},
            "policies": {k: dict(v) for k, v in POLICIES.items()},
            "_resources": [resource_hint("policy:POL-1", str(POLICIES["POL-1"]["version"]), "policy-admin")],
        }
        return StateSnapshot(state=state, version=str(POLICIES["POL-1"]["version"]), source="claims-memory")

    def version(self, resource_id: str) -> str | None:
        if resource_id in ("policy:POL-1", "POL-1"):
            return str(POLICIES["POL-1"]["version"])
        return str(CLAIMS.get(resource_id, {}).get("payout_cents"))


def claim_paid(state: StateSnapshot) -> InvariantOutcome:
    c = (state.get("claims") or {}).get("CLM-77") or {}
    ok = c.get("status") == "paid" and c.get("payout_cents", 0) > 0
    return InvariantOutcome(ok, expected="paid", actual=c.get("status"))


def payout_within_reserve(state: StateSnapshot) -> bool:
    c = (state.get("claims") or {}).get("CLM-77") or {}
    p = (state.get("policies") or {}).get("POL-1") or {}
    amount = c.get("payout_cents") or c.get("recommended_cents") or 0
    return amount <= (p.get("reserve_cents") or 0)


async def mutate_after_delay(seconds: float) -> None:
    await asyncio.sleep(seconds)
    mutate_policy_to_v5()
    print("policy POL-1 mutated to version 5 (reserve_cents=5000)", flush=True)


async def run_once(*, mutate: bool) -> int:
    reset_world()
    os.environ["AXIOM_MUTATE_POLICY"] = "1" if mutate else "0"
    agent = Agent(
        name="claims_agent",
        model=MockModel(
            script=[
                ModelTurn(tool_calls=[("lookup_claim", {"claim_id": "CLM-77"})]),
                ModelTurn(tool_calls=[("check_coverage", {"claim_id": "CLM-77"})]),
                ModelTurn(tool_calls=[("authorize_payout", {"claim_id": "CLM-77", "amount_cents": 15000})]),
                ModelTurn(content="Payout authorized."),
            ]
        ),
        tools=[lookup_claim, check_coverage, authorize_payout],
        permissions=["claims:payout"],
    )
    goal = Goal(description="Pay covered claim CLM-77", success=claim_paid, invariants=[payout_within_reserve], max_steps=8)
    print("Waiting for approval in Studio (/approvals) after the payout tool is proposed...", flush=True)
    if mutate:
        print("This run will mutate policy POL-1 4→5 while approval is pending.", flush=True)
    async with Runtime(agent, state_provider=ClaimsState()) as rt:
        mutator = asyncio.create_task(mutate_after_delay(1.5)) if mutate else None
        result = await rt.run(goal, input={"claim_id": "CLM-77", "mutate_policy": mutate})
        if mutator:
            mutator.cancel()
    print(result.summary())
    print(f"policy_version: {POLICIES['POL-1']['version']}")
    print(f"claim_status: {CLAIMS['CLM-77']['status']}")
    print(f"payout_cents: {CLAIMS['CLM-77']['payout_cents']}")
    return 0 if result.verified else 1


def main() -> int:
    logging.basicConfig(level=os.environ.get("AXIOM_LOG", "INFO"))
    p = argparse.ArgumentParser(description="Axiom insurance claims demo")
    p.add_argument("--mutate-policy", action="store_true", help="Change POL-1 from v4 to v5 while approval is pending")
    args = p.parse_args()
    return asyncio.run(run_once(mutate=args.mutate_policy))


if __name__ == "__main__":
    sys.exit(main())
