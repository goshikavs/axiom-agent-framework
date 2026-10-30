"""Simulated insurance records. Policy version is authoritative business state."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class ClaimWorld:
    policies: dict[str, dict[str, Any]] = field(default_factory=dict)
    claims: dict[str, dict[str, Any]] = field(default_factory=dict)
    fraud: dict[str, dict[str, Any]] = field(default_factory=dict)
    payouts: dict[str, dict[str, Any]] = field(default_factory=dict)

    def reset(self) -> None:
        self.policies = {"POL-1": {"id": "POL-1", "version": 4, "coverage": True, "reserve_cents": 15000}}
        self.claims = {
            "CLM-77": {
                "id": "CLM-77",
                "policy_id": "POL-1",
                "status": "open",
                "recommended_cents": 15000,
                "payout_cents": 0,
            }
        }
        self.fraud = {}
        self.payouts = {}

    def mutate_policy_v5(self) -> None:
        self.policies["POL-1"] = {"id": "POL-1", "version": 5, "coverage": True, "reserve_cents": 5000}

    def snapshot(self) -> dict[str, Any]:
        return {
            "policies": dict(self.policies),
            "claims": dict(self.claims),
            "fraud": dict(self.fraud),
            "payouts": dict(self.payouts),
        }


WORLD = ClaimWorld()
WORLD.reset()
