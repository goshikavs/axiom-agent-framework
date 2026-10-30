"""Unit-level checks that do not require the runtime process."""

from main import GREETINGS, GreetingState, ada_was_greeted, no_more_than_one_greeting, record_greeting
from axiom_ai.state import SnapshotContext, StateSnapshot


def test_tool_and_predicates() -> None:
    GREETINGS.clear()
    record_greeting("Ada", "Hello, Ada!")
    snap = GreetingState().snapshot(SnapshotContext(run_id="r", reason="test", step=1))
    assert ada_was_greeted(snap)
    assert no_more_than_one_greeting(snap)
    empty = StateSnapshot(state={"greetings": {}})
    assert not ada_was_greeted(empty)
