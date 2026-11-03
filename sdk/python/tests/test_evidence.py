from goshikq_ai import DecisionRef, Evidence, with_evidence
from goshikq_ai.evidence import resource_hint


def test_evidence_omits_source_blob() -> None:
    e = Evidence(source_system="policy-admin", resource_id="policy:POL-1", resource_version="4", trust="authoritative")
    d = e.to_dict()
    assert "content" not in d
    assert d["resource_version"] == "4"
    assert d["trust"] == "authoritative"


def test_decision_can_reference_multiple_evidence_ids() -> None:
    dec = DecisionRef(
        id="dec:coverage:CLM-77",
        kind="coverage",
        depends_on=["policy:POL-1", "claim:CLM-77"],
        action="act:authorize_payout",
        payload={"resource_id": "policy:POL-1", "amount_cents": 15000},
    )
    attached = with_evidence({"covered": True}, [Evidence(source_system="policy-admin", resource_id="policy:POL-1", resource_version="4"), Evidence(source_system="claims", resource_id="claim:CLM-77", resource_version="1")], dec)
    assert len(attached["_evidence"]) == 2
    assert attached["_decision"]["depends_on"] == ["policy:POL-1", "claim:CLM-77"]
    assert attached["covered"] is True


def test_resource_hint() -> None:
    h = resource_hint("order:ORD-2048", "3", "orders-db")
    assert h["resource_id"] == "order:ORD-2048"
    assert h["version"] == "3"
