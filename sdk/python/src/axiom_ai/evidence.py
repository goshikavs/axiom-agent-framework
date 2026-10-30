"""Versioned evidence and decision hints attached to tool results.

These are *explicitly registered* by tools and state providers. Axiom does not
infer what a model internally considered. Full source documents are not required;
a fingerprint or resource version is enough.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Literal

Trust = Literal["authoritative", "derived", "untrusted", "inaccessible"]
DecisionStatus = Literal["PENDING", "VALID", "INVALID", "INCONCLUSIVE", "PENDING_REVALIDATION"]


@dataclass
class Evidence:
    source_system: str
    resource_id: str
    resource_version: str | None = None
    etag: str | None = None
    fingerprint: str | None = None
    trust: Trust = "authoritative"
    access_scope: str | None = None
    valid_from: datetime | None = None
    valid_until: datetime | None = None
    id: str | None = None
    keys: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "source_system": self.source_system,
            "resource_id": self.resource_id,
            "trust": self.trust,
        }
        if self.resource_version is not None:
            d["resource_version"] = self.resource_version
        if self.etag is not None:
            d["etag"] = self.etag
        if self.fingerprint is not None:
            d["fingerprint"] = self.fingerprint
        if self.access_scope is not None:
            d["access_scope"] = self.access_scope
        if self.valid_from is not None:
            d["valid_from"] = self.valid_from.isoformat()
        if self.valid_until is not None:
            d["valid_until"] = self.valid_until.isoformat()
        if self.id is not None:
            d["id"] = self.id
        if self.keys:
            d["keys"] = list(self.keys)
        return d


@dataclass
class DecisionRef:
    id: str
    kind: str = "unknown"
    depends_on: list[str] = field(default_factory=list)
    payload: dict[str, Any] = field(default_factory=dict)
    action: str | None = None

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {
            "id": self.id,
            "kind": self.kind,
            "depends_on": list(self.depends_on),
            "payload": dict(self.payload),
        }
        if self.action:
            d["action"] = self.action
        return d


def with_evidence(result: dict[str, Any], evidence: list[Evidence], decision: DecisionRef | None = None) -> dict[str, Any]:
    """Attach evidence/decision metadata to a tool result without embedding source blobs."""
    out = dict(result)
    out["_evidence"] = [e.to_dict() for e in evidence]
    if decision is not None:
        out["_decision"] = decision.to_dict()
    return out


def resource_hint(resource_id: str, version: str | None, source_system: str | None = None) -> dict[str, Any]:
    d: dict[str, Any] = {"resource_id": resource_id, "version": version}
    if source_system:
        d["source_system"] = source_system
    return d
