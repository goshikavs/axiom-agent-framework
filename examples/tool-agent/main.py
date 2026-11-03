"""tool-agent: typed tool calling with invalid-argument handling.

The mock model first proposes a bad payload (rejected by schema validation in the
runtime), then a valid call. The goal verifies that the inventory was read.
"""

from __future__ import annotations

import asyncio
import logging
import os
import sys

from goshikq_ai import Agent, Goal, Runtime, StateSnapshot, tool
from goshikq_ai.models import MockModel, ModelTurn
from goshikq_ai.state import SnapshotContext

INVENTORY = {"SKU-9": {"name": "ceramic mug", "on_hand": 12}}
LOOKUPS: list[str] = []


@tool(effect="read_only", risk="low")
def lookup_sku(sku: str) -> dict:
    """Return inventory for an SKU. `sku` is required and must be a string."""
    LOOKUPS.append(sku)
    item = INVENTORY.get(sku)
    if item is None:
        return {"found": False, "sku": sku}
    return {"found": True, "sku": sku, **item}


class InventoryState:
    def snapshot(self, context: SnapshotContext) -> StateSnapshot:
        return StateSnapshot(state={"lookups": list(LOOKUPS), "inventory": dict(INVENTORY)}, version=str(len(LOOKUPS)), source="inventory-memory")

    def version(self, resource_id: str) -> str | None:
        return str(len(LOOKUPS))


def sku_was_looked_up(state: StateSnapshot) -> bool:
    return "SKU-9" in (state.get("lookups") or [])


model = MockModel(
    script=[
        ModelTurn(tool_calls=[("lookup_sku", {"sku": 9})]),  # invalid: sku must be a string
        ModelTurn(tool_calls=[("lookup_sku", {"sku": "SKU-9"})]),
        ModelTurn(content="SKU-9 is a ceramic mug with 12 on hand."),
    ]
)

agent = Agent(name="tool-agent", model=model, instructions="Look up SKU-9.", tools=[lookup_sku])
goal = Goal(description="SKU-9 has been looked up in inventory", success=sku_was_looked_up, max_steps=6)


async def main() -> int:
    logging.basicConfig(level=os.environ.get("GOSHIKQ_LOG", "WARNING"))
    LOOKUPS.clear()
    async with Runtime(agent, state_provider=InventoryState()) as rt:
        result = await rt.run(goal, input={"sku": "SKU-9"})
    print(result.summary())
    return 0 if result.verified else 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
