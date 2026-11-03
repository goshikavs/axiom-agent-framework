"""Unit-level checks that do not require the runtime process."""

from __future__ import annotations

import importlib.util
from pathlib import Path

from goshikq_ai.state import SnapshotContext, StateSnapshot


def _load_example_main():
    path = Path(__file__).resolve().parent / "main.py"
    spec = importlib.util.spec_from_file_location("goshikq_example_hello_agent_main", path)
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_main = _load_example_main()
GREETINGS = _main.GREETINGS
GreetingState = _main.GreetingState
ada_was_greeted = _main.ada_was_greeted
no_more_than_one_greeting = _main.no_more_than_one_greeting
record_greeting = _main.record_greeting


def test_tool_and_predicates() -> None:
    GREETINGS.clear()
    record_greeting("Ada", "Hello, Ada!")
    snap = GreetingState().snapshot(SnapshotContext(run_id="r", reason="test", step=1))
    assert ada_was_greeted(snap)
    assert no_more_than_one_greeting(snap)
    empty = StateSnapshot(state={"greetings": {}})
    assert not ada_was_greeted(empty)
