"""Simulated retail records shared by the refund team. Authoritative state only."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class TeamStore:
    orders: dict[str, dict[str, Any]] = field(default_factory=dict)
    inspections: dict[str, dict[str, Any]] = field(default_factory=dict)
    decisions: dict[str, dict[str, Any]] = field(default_factory=dict)
    refunds: dict[str, dict[str, Any]] = field(default_factory=dict)

    def reset(self) -> None:
        self.orders = {
            "ORD-2048": {
                "id": "ORD-2048",
                "status": "paid",
                "amount_cents": 4200,
                "eligible": True,
                "version": 1,
            }
        }
        self.inspections = {}
        self.decisions = {}
        self.refunds = {}

    def snapshot(self) -> dict[str, Any]:
        return {
            "orders": dict(self.orders),
            "inspections": dict(self.inspections),
            "decisions": dict(self.decisions),
            "refunds": dict(self.refunds),
        }


STORE = TeamStore()
STORE.reset()
