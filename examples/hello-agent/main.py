"""hello-agent: the smallest verified Axiom run.

Path exercised: Python SDK -> Axiom Rust runtime -> deterministic mock model -> tool
execution -> goal verification against a StateSnapshot -> PostgreSQL events.

Run:   python main.py            (runtime must be listening on AXIOM_RUNTIME_URL, default 127.0.0.1:7400)
"""

from __future__ import annotations

import asyncio
import logging
import os
import sys

from axiom_ai import Agent, Goal, Runtime, StateSnapshot, tool
from axiom_ai.models import MockModel, ModelTurn
from axiom_ai.state import SnapshotContext

# A tiny in-memory "system of record". Real apps read a DB or API here.
GREETINGS: dict[str, str] = {}


@tool(effect="idempotent_write", idempotent=True, risk="low")
def record_greeting(name: str, message: str) -> dict:
    """Store a greeting for `name`. Writing the same greeting twice is a no-op."""
    GREETINGS[name] = message
    return {"stored": True, "name": name}


class GreetingState:
    """StateProvider: the runtime asks this for authoritative state, never the model."""

    def snapshot(self, context: SnapshotContext) -> StateSnapshot:
        return StateSnapshot(state={"greetings": dict(GREETINGS)}, version=str(len(GREETINGS)), source="greetings-memory")

    def version(self, resource_id: str) -> str | None:
        return str(len(GREETINGS))


def ada_was_greeted(state: StateSnapshot) -> bool:
    return isinstance(state.get("greetings.Ada"), str) and "Ada" in state.get("greetings.Ada")


def no_more_than_one_greeting(state: StateSnapshot) -> bool:
    return len(state.get("greetings", {})) <= 1


model = MockModel(
    script=[
        ModelTurn(tool_calls=[("record_greeting", {"name": "Ada", "message": "Hello, Ada!"})]),
        ModelTurn(content="Greeted Ada."),
    ]
)

agent = Agent(name="hello-agent", model=model, instructions="Greet the person named in the input.", tools=[record_greeting])
goal = Goal(
    description="Ada has been greeted exactly once",
    success=ada_was_greeted,
    invariants=[no_more_than_one_greeting],
    max_steps=4,
)


async def main() -> int:
    logging.basicConfig(level=os.environ.get("AXIOM_LOG", "WARNING"))
    async with Runtime(agent, state_provider=GreetingState()) as rt:
        result = await rt.run(goal, input={"name": "Ada"})
    print(result.summary())
    return 0 if result.verified else 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
