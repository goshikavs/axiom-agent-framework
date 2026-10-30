"""Example B — insurance claim team. A policy version change invalidates payout.

Children A–C may complete. Payout (D) and the parent goal still read the current
policy version from the store. Child SUCCESS is not parent VERIFIED.
"""

from __future__ import annotations

import argparse
import asyncio
import os
import sys

from axiom_ai import Agent, AgentGroup, AgentTask, CoordinationPolicy, Goal, Runtime, StateSnapshot, tool
from axiom_ai.models import MockModel, ModelTurn
from axiom_ai.state import SnapshotContext

from store import WORLD

WORLD.reset()


@tool(effect="read_only")
def check_coverage(claim_id: str) -> dict:
    claim = WORLD.claims[claim_id]
    policy = WORLD.policies[claim["policy_id"]]
    return {"covered": bool(policy["coverage"]), "policy_version": policy["version"], "reserve_cents": policy["reserve_cents"]}


@tool(effect="read_only")
def assess_claim(claim_id: str) -> dict:
    c = WORLD.claims[claim_id]
    return {"claim_id": claim_id, "recommended_cents": c["recommended_cents"], "status": c["status"]}


@tool(effect="read_only")
def fraud_check(claim_id: str) -> dict:
    WORLD.fraud[claim_id] = {"claim_id": claim_id, "flags": [], "clear": True}
    return WORLD.fraud[claim_id]


@tool(effect="non_idempotent_write", risk="critical", permissions=["claims:payout"])
def prepare_payout(claim_id: str, amount_cents: int) -> dict:
    claim = WORLD.claims[claim_id]
    policy = WORLD.policies[claim["policy_id"]]
    # Current business invariant: payout cannot exceed the live reserve.
    if amount_cents > int(policy["reserve_cents"]):
        return {"authorized": False, "reason": "reserve_changed", "policy_version": policy["version"]}
    WORLD.payouts[claim_id] = {"claim_id": claim_id, "amount_cents": amount_cents, "status": "settled", "policy_version": policy["version"]}
    claim["payout_cents"] = amount_cents
    claim["status"] = "paid"
    return WORLD.payouts[claim_id]


def payout_settled(snap: StateSnapshot) -> bool:
    payout = (snap.get("payouts") or {}).get("CLM-77") or {}
    policy = (snap.get("policies") or {}).get("POL-1") or {}
    return payout.get("status") == "settled" and payout.get("policy_version") == policy.get("version")


def child_ok(_snap: StateSnapshot) -> bool:
    return True


class WorldProvider:
    def snapshot(self, context: SnapshotContext) -> StateSnapshot:
        snap = WORLD.snapshot()
        return StateSnapshot(state=snap, version=str(snap["policies"]["POL-1"]["version"]), source="policy-admin")


def _agent(name: str, tools: list, calls: list[tuple[str, dict]]) -> Agent:
    turns = [ModelTurn(tool_calls=[c]) for c in calls] + [ModelTurn(content="done")]
    return Agent(name=name, model=MockModel(script=turns), tools=tools, permissions=["claims:payout"])


async def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mutate-policy", action="store_true", help="Flip POL-1 to v5 after children A–C and before payout")
    args = parser.parse_args()
    WORLD.reset()
    provider = WorldProvider()
    child_goal = Goal(description="Complete assigned claim subtask", success=child_ok)
    coverage = _agent("policy-coverage", [check_coverage], [("check_coverage", {"claim_id": "CLM-77"})])
    assessment = _agent("claim-assessment", [assess_claim], [("assess_claim", {"claim_id": "CLM-77"})])
    fraud = _agent("fraud-rules", [fraud_check], [("fraud_check", {"claim_id": "CLM-77"})])
    payout = _agent("payout", [prepare_payout], [("prepare_payout", {"claim_id": "CLM-77", "amount_cents": 15000})])
    supervisor = Agent(
        name="claim-supervisor",
        model=MockModel(script=[ModelTurn(content="verify live policy and payout")]),
        permissions=["claims:payout"],
    )

    # Custom sequence so we can mutate between assessment and payout.
    group = AgentGroup(
        supervisor,
        policy=CoordinationPolicy(mode="sequential"),
        state_provider=provider,
        endpoint=os.environ.get("AXIOM_RUNTIME_URL"),
    )
    parent_goal = Goal(description="Pay CLM-77 only if the live policy still authorizes the reserve", success=payout_settled)
    created = await group.parent_runtime.create_deferred(parent_goal, input={"claim_id": "CLM-77"}, metadata={"example": "multi-agent-insurance", "coordination": group.policy.to_dict()})
    parent_id = str(created["run_id"])

    async def run_child(task: AgentTask) -> None:
        body = {
            "agent": task.agent.to_definition(),
            "goal": task.goal.to_definition(),
            "input": task.input,
            "has_state_provider": True,
            "source_framework": "axiom-python",
            "role": task.role,
        }
        child = await group.client.create_task(parent_id, body)
        rt = Runtime(task.agent, endpoint=group.client.endpoint, state_provider=provider)
        rt.client = group.client
        await rt.attach(str(child["child_run_id"]), task.goal)

    await run_child(AgentTask(coverage, child_goal, role="coverage"))
    await run_child(AgentTask(assessment, child_goal, role="assessment"))
    await run_child(AgentTask(fraud, child_goal, role="fraud"))
    if args.mutate_policy:
        WORLD.mutate_policy_v5()
        print("mutated POL-1 to version 5 (reserve 5000) before payout")
    await run_child(AgentTask(payout, child_goal, role="payout"))
    await group.client.start_run(parent_id)
    parent = await group.parent_runtime.attach(parent_id, parent_goal)
    print(parent.summary())
    print(f"policy_version={WORLD.policies['POL-1']['version']} payouts={WORLD.payouts}")
    print(f"parent_verified={parent.verified}")
    return 0 if (parent.verified and not args.mutate_policy) or (args.mutate_policy and not parent.verified) else 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
