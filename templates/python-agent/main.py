"""Minimal Python agent template for Axiom.

Requires axiom-server on AXIOM_RUNTIME_URL (default http://127.0.0.1:7400).
"""

from __future__ import annotations

import asyncio
import logging
import os

from axiom_ai import Agent, Goal, Runtime, StateSnapshot, tool
from axiom_ai.models import MockModel, ModelTurn
from axiom_ai.state import SnapshotContext

STORE: dict[str, str] = {}


@tool(effect="idempotent_write", idempotent=True, risk="low")
def record_greeting(name: str, message: str) -> dict:
    STORE[name] = message
    return {"stored": True, "name": name}


class GreetingState:
    def snapshot(self, context: SnapshotContext) -> StateSnapshot:
        return StateSnapshot(state={"greetings": dict(STORE)}, version=str(len(STORE)), source="template-memory")

    def version(self, resource_id: str) -> str | None:
        return str(len(STORE))


async def main() -> None:
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    agent = Agent(
        name="hello-template",
        model=MockModel(script=[ModelTurn(tool_calls=[{"name": "record_greeting", "arguments": {"name": "world", "message": "hello"}}])]),
        tools=[record_greeting],
        state_provider=GreetingState(),
    )
    result = await Runtime(agent, url=os.environ.get("AXIOM_RUNTIME_URL", "http://127.0.0.1:7400")).run(
        Goal(description="Greet world", success_predicate="greeting_stored")
    )
    print(result.status, result.verification_status, result.run_id)


if __name__ == "__main__":
    asyncio.run(main())
