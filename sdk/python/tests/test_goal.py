import asyncio

from axiom_ai.goal import Goal, InvariantOutcome
from axiom_ai.state import StateSnapshot


def paid(state: StateSnapshot) -> bool:
    return state.get("order.status") == "refunded"


def amount_ok(state: StateSnapshot) -> InvariantOutcome:
    amount = state.get("refund.amount", 0)
    ok = 0 < amount <= 100
    return InvariantOutcome(ok, expected={"max": 100}, actual=amount, reason=None if ok else "over limit")


def test_evaluate_success_and_invariant() -> None:
    goal = Goal(description="refund", success=paid, invariants=[amount_ok])
    snap = StateSnapshot(state={"order": {"status": "refunded"}, "refund": {"amount": 40}}, version="1")
    result = asyncio.run(goal.evaluate(snap, evaluate_success=True))
    assert result["success"] is True
    assert result["invariants"][0]["satisfied"] is True

    bad = StateSnapshot(state={"order": {"status": "paid"}, "refund": {"amount": 500}}, version="2")
    result = asyncio.run(goal.evaluate(bad, evaluate_success=True))
    assert result["success"] is False
    assert result["invariants"][0]["satisfied"] is False


def test_definition_names() -> None:
    goal = Goal(description="refund", success=paid, invariants=[amount_ok], max_steps=5)
    d = goal.to_definition()
    assert d["success_predicate"] == "paid"
    assert d["invariants"] == ["amount_ok"]
    assert d["max_steps"] == 5
    assert d["verification_policy"] == "required"


def test_external_wait_seconds_on_definition() -> None:
    goal = Goal(description="refund", success=paid, external_wait_seconds=20)
    assert goal.to_definition()["external_wait_seconds"] == 20
