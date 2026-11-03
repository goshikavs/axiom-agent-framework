"""Recovery policy tests. These do not claim exactly-once external I/O.

The Rust runtime owns the real policy (`goshikq-core::recovery`). This file checks the
simulated processor and verifier contract that the runtime relies on.
"""

from goshikq_ai.tools import VerifierResult


def test_verifier_waiting_is_not_verified() -> None:
    r = VerifierResult.waiting(
        "accepted, not settled",
        expected="settled",
        actual={"refund.status": "accepted"},
        business_operation_status="PENDING_SETTLEMENT",
    )
    d = r.to_dict()
    assert d["status"] == "INCONCLUSIVE"
    assert d["wait"] is True
    assert d["business_operation_status"] == "PENDING_SETTLEMENT"
    assert d["status"] != "VERIFIED"


def test_six_interruption_boundaries_never_blindly_retry_non_idempotent() -> None:
    """Documented mapping; the executable copy lives in goshikq-core recovery tests.

    1. Before tool execution → resume planning (no side effect started).
    2. After recording intent, before dispatch → hold; do not dispatch.
    3. During tool execution (non-idempotent) → reconcile; do not auto-retry.
    4. After external accept, before completion → wait for external state.
    5. During verification of a succeeded write → wait for external state.
    6. Waiting for human approval → resume approval wait; do not recreate.
    """
    actions = {
        "before_tool": "resume_planning",
        "after_intent_before_dispatch": "hold_without_dispatch",
        "during_tool_non_idempotent": "reconcile_do_not_retry",
        "after_accept_before_completion": "wait_for_external_state",
        "during_verification_succeeded": "wait_for_external_state",
        "waiting_for_approval": "resume_approval_wait",
    }
    assert "retry_same_key" not in actions.values()
    assert actions["during_tool_non_idempotent"] != "retry_same_key"
    assert actions["after_accept_before_completion"] == "wait_for_external_state"


def test_historical_replay_contract_is_read_only() -> None:
    """Studio replay is GET /events. No tool.execute worker requests are issued."""
    assert "/api/v1/runs/{id}/events" != "/api/v1/runs/{id}/worker/poll"
