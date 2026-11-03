"""Simulated order database and payment processor.

A successful HTTP-like acceptance from the processor is NOT proof the refund settled.
Callers must poll `get_refund` and read `orders[id].status` from this module — the same
objects the StateProvider exposes to the runtime.
"""

from __future__ import annotations

import json
import os
import time
from dataclasses import dataclass, field
from datetime import UTC, datetime, timedelta
from typing import Any, Literal


class Fault(str):
    NONE = "none"
    PROCESSOR_REJECT = "processor_reject"
    DELAYED_SETTLEMENT = "delayed_settlement"
    NEVER_SETTLES = "never_settles"
    DUPLICATE = "duplicate"
    STALE_ORDER = "stale_order"
    MISSING_EVIDENCE = "missing_evidence"
    RETURN_CHANGED = "return_changed"


@dataclass
class Order:
    id: str
    customer: str
    amount_cents: int
    currency: str
    paid_at: str
    status: Literal["paid", "return_started", "refund_pending", "refunded", "refund_failed"]
    eligible: bool
    version: int = 1
    refund_id: str | None = None
    return_status: str = "received"


@dataclass
class Refund:
    id: str
    order_id: str
    amount_cents: int
    status: Literal["accepted", "settled", "rejected"]
    idempotency_key: str
    created_at: str
    settled_at: str | None = None
    reject_reason: str | None = None


@dataclass
class SimulatedStore:
    orders: dict[str, Order] = field(default_factory=dict)
    refunds: dict[str, Refund] = field(default_factory=dict)
    refunds_by_key: dict[str, str] = field(default_factory=dict)
    clock: datetime = field(default_factory=lambda: datetime(2026, 4, 20, 12, 0, tzinfo=UTC))
    fault: str = Fault.NONE
    settlement_delay: timedelta = timedelta(0)
    settlement_delay_seconds: float = 0.0
    authorized_limit_cents: int = 20_000
    _accepted_mono: dict[str, float] = field(default_factory=dict)
    submit_count: int = 0
    eligibility_checked: bool = False
    eligibility_order_id: str | None = None

    def persist(self) -> None:
        """Atomic JSON snapshot so a worker crash does not lose processor records."""
        path = os.environ.get("GOSHIKQ_STORE_PATH")
        if not path:
            return
        payload = {
            "orders": {k: o.__dict__.copy() for k, o in self.orders.items()},
            "refunds": {k: r.__dict__.copy() for k, r in self.refunds.items()},
            "refunds_by_key": dict(self.refunds_by_key),
            "fault": self.fault,
            "settlement_delay_seconds": self.settlement_delay_seconds,
            "authorized_limit_cents": self.authorized_limit_cents,
            "submit_count": self.submit_count,
            "eligibility_checked": self.eligibility_checked,
            "eligibility_order_id": self.eligibility_order_id,
            "accepted_mono": dict(self._accepted_mono),
            "clock": self.clock.isoformat(),
        }
        tmp = f"{path}.tmp"
        os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(payload, f)
        os.replace(tmp, path)

    @classmethod
    def load(cls, path: str) -> SimulatedStore | None:
        if not os.path.exists(path):
            return None
        with open(path, encoding="utf-8") as f:
            payload = json.load(f)
        store = cls(fault=payload.get("fault", Fault.NONE))
        store.settlement_delay_seconds = float(payload.get("settlement_delay_seconds") or 0)
        store.authorized_limit_cents = int(payload.get("authorized_limit_cents") or 20_000)
        store.submit_count = int(payload.get("submit_count") or 0)
        store.refunds_by_key = dict(payload.get("refunds_by_key") or {})
        store._accepted_mono = {k: float(v) for k, v in (payload.get("accepted_mono") or {}).items()}
        if payload.get("clock"):
            store.clock = datetime.fromisoformat(payload["clock"])
        store.eligibility_checked = bool(payload.get("eligibility_checked") or False)
        store.eligibility_order_id = payload.get("eligibility_order_id")
        for oid, raw in (payload.get("orders") or {}).items():
            raw.setdefault("return_status", "received")
            store.orders[oid] = Order(**raw)
        for rid, raw in (payload.get("refunds") or {}).items():
            store.refunds[rid] = Refund(**raw)
        return store

    def maybe_settle_delayed(self) -> None:
        """Advance delayed settlements based on wall clock. Replay of recorded events does not call this."""
        if self.fault == Fault.NEVER_SETTLES:
            return
        if not self._accepted_mono:
            return
        now = time.monotonic()
        for rid, t0 in list(self._accepted_mono.items()):
            if now - t0 >= self.settlement_delay_seconds:
                self.settle_now(rid)
                del self._accepted_mono[rid]

    def snapshot(self) -> dict[str, Any]:
        self.maybe_settle_delayed()
        if self.fault == Fault.RETURN_CHANGED and self.eligibility_checked:
            oid = self.eligibility_order_id or os.environ.get("GOSHIKQ_REFUND_ORDER") or "ORD-2048"
            order = self.orders.get(oid)
            if order is not None and order.return_status != "rejected":
                order.return_status = "rejected"
                order.eligible = False
                order.version += 1
                self.persist()
        orders = {k: o.__dict__.copy() for k, o in self.orders.items()}
        resources = [{"resource_id": f"order:{oid}", "version": str(o.get("version")), "source_system": "orders-db"} for oid, o in orders.items()]
        return {
            "orders": orders,
            "refunds": {k: r.__dict__.copy() for k, r in self.refunds.items()},
            "authorized_limit_cents": self.authorized_limit_cents,
            "fault": self.fault,
            "_resources": resources,
        }

    def lookup_order(self, order_id: str) -> dict[str, Any]:
        order = self.orders.get(order_id)
        if order is None:
            return {"found": False, "order_id": order_id}
        if self.fault == Fault.STALE_ORDER:
            stale = order.__dict__.copy()
            stale["status"] = "paid"
            stale["stale"] = True
            return {"found": True, "stale": True, **stale}
        return {"found": True, **order.__dict__.copy()}

    def check_eligibility(self, order_id: str) -> dict[str, Any]:
        order = self.orders.get(order_id)
        if order is None:
            return {"eligible": False, "reason": "unknown_order"}
        self.eligibility_checked = True
        self.eligibility_order_id = order_id
        if order.return_status == "rejected":
            return {"eligible": False, "reason": "return_rejected", "order_status": order.status, "return_status": order.return_status, "order_version": order.version}
        if not order.eligible:
            return {"eligible": False, "reason": "policy_ineligible", "order_status": order.status, "return_status": order.return_status}
        if order.status in ("refunded", "refund_pending"):
            return {"eligible": False, "reason": f"already_{order.status}", "refund_id": order.refund_id}
        return {"eligible": True, "amount_cents": order.amount_cents, "currency": order.currency}

    def submit_refund(self, order_id: str, amount_cents: int, idempotency_key: str) -> dict[str, Any]:
        self.submit_count += 1
        if existing := self.refunds_by_key.get(idempotency_key):
            r = self.refunds[existing]
            self.persist()
            return {"duplicate": True, "refund": r.__dict__.copy()}
        order = self.orders.get(order_id)
        if order is None:
            return {"accepted": False, "reason": "unknown_order"}
        if amount_cents > self.authorized_limit_cents:
            return {"accepted": False, "reason": "over_authorized_limit"}
        if self.fault == Fault.PROCESSOR_REJECT:
            rid = f"rf-{len(self.refunds)+1}"
            refund = Refund(
                id=rid,
                order_id=order_id,
                amount_cents=amount_cents,
                status="rejected",
                idempotency_key=idempotency_key,
                created_at=self.clock.isoformat(),
                reject_reason="processor_declined",
            )
            self.refunds[rid] = refund
            self.refunds_by_key[idempotency_key] = rid
            order.status = "refund_failed"
            order.version += 1
            self.persist()
            return {"accepted": False, "reason": "processor_declined", "refund": refund.__dict__.copy()}
        rid = f"rf-{len(self.refunds)+1}"
        pending = self.fault in (Fault.DELAYED_SETTLEMENT, Fault.NEVER_SETTLES)
        settle_at = None if pending else self.clock.isoformat()
        status: Literal["accepted", "settled", "rejected"] = "accepted" if pending else "settled"
        refund = Refund(
            id=rid,
            order_id=order_id,
            amount_cents=amount_cents,
            status=status,
            idempotency_key=idempotency_key,
            created_at=self.clock.isoformat(),
            settled_at=settle_at,
        )
        self.refunds[rid] = refund
        self.refunds_by_key[idempotency_key] = rid
        if status == "accepted" and self.fault != Fault.NEVER_SETTLES:
            self._accepted_mono[rid] = time.monotonic()
        order.refund_id = rid
        order.status = "refunded" if status == "settled" else "refund_pending"
        order.version += 1
        self.persist()
        return {"accepted": True, "settled": status == "settled", "refund": refund.__dict__.copy()}

    def get_refund(self, refund_id: str) -> dict[str, Any]:
        self.maybe_settle_delayed()
        r = self.refunds.get(refund_id)
        if r is None:
            return {"found": False, "refund_id": refund_id}
        if r.status == "accepted" and self.fault not in (Fault.DELAYED_SETTLEMENT, Fault.NEVER_SETTLES):
            r.status = "settled"
            r.settled_at = self.clock.isoformat()
            if order := self.orders.get(r.order_id):
                order.status = "refunded"
                order.version += 1
        return {"found": True, **r.__dict__.copy()}

    def settle_now(self, refund_id: str) -> None:
        """Test helper: complete a delayed settlement."""
        r = self.refunds[refund_id]
        r.status = "settled"
        r.settled_at = self.clock.isoformat()
        order = self.orders[r.order_id]
        order.status = "refunded"
        order.version += 1
        self.persist()


def demo_store(fault: str = Fault.NONE) -> SimulatedStore:
    path = os.environ.get("GOSHIKQ_STORE_PATH")
    if path:
        loaded = SimulatedStore.load(path)
        if loaded is not None:
            loaded.fault = fault
            if fault == Fault.DELAYED_SETTLEMENT and loaded.settlement_delay_seconds <= 0:
                loaded.settlement_delay_seconds = 2.0
            return loaded
    store = SimulatedStore(fault=fault)
    if fault == Fault.DELAYED_SETTLEMENT:
        store.settlement_delay_seconds = 2.0
    if fault == Fault.NEVER_SETTLES:
        store.settlement_delay_seconds = 1e9
    store.orders["ORD-2048"] = Order(
        id="ORD-2048",
        customer="Ada Lovelace",
        amount_cents=4800,
        currency="USD",
        paid_at="2026-04-01T10:00:00+00:00",
        status="paid",
        eligible=True,
        version=3,
    )
    store.orders["ORD-LOCKED"] = Order(
        id="ORD-LOCKED",
        customer="Grace Hopper",
        amount_cents=9900,
        currency="USD",
        paid_at="2026-01-01T10:00:00+00:00",
        status="paid",
        eligible=False,
        version=1,
    )
    store.persist()
    return store
