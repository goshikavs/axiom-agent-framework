"""Authoritative state access."""

from __future__ import annotations

from collections.abc import Awaitable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any, Protocol, runtime_checkable


@dataclass
class StateSnapshot:
    """A typed view of authoritative external state at a point in time.

    ``version`` should be an explicit version/etag when the source offers one. Version
    checks detect staleness of *that* resource; they do not provide atomicity across
    unrelated services.
    """

    state: dict[str, Any]
    version: str | None = None
    source: str = ""
    captured_at: datetime = field(default_factory=lambda: datetime.now(UTC))

    def get(self, path: str, default: Any = None) -> Any:
        """Dotted-path lookup, e.g. ``snapshot.get("order.status")``."""
        cur: Any = self.state
        for part in path.split("."):
            if isinstance(cur, dict) and part in cur:
                cur = cur[part]
            else:
                return default
        return cur

    def to_dict(self) -> dict[str, Any]:
        return {"state": self.state, "version": self.version, "source": self.source, "captured_at": self.captured_at.isoformat()}


@dataclass
class SnapshotContext:
    run_id: str
    reason: str
    step: int | None


@runtime_checkable
class StateProvider(Protocol):
    """Reads authoritative external state for goal evaluation and action verification."""

    def snapshot(self, context: SnapshotContext) -> StateSnapshot | Awaitable[StateSnapshot]: ...

    def version(self, resource_id: str) -> str | None | Awaitable[str | None]: ...
