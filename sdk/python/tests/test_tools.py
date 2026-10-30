from axiom_ai.errors import ConfigurationError, ToolError
from axiom_ai.tools import Effect, VerifierResult, tool


@tool(effect="read_only")
def lookup(order_id: str, include_history: bool = False) -> dict:
    """Look up an order."""
    return {"order_id": order_id, "history": include_history}


def test_schema_from_signature() -> None:
    schema = lookup.input_schema
    assert schema["type"] == "object"
    assert schema["additionalProperties"] is False
    assert "order_id" in schema["properties"]
    assert "order_id" in schema["required"]
    assert "include_history" not in schema.get("required", [])
    assert "idempotency_key" not in schema["properties"]


def test_validate_rejects_unknown_fields() -> None:
    try:
        lookup.validate_arguments({"order_id": "A", "extra": 1})
        raise AssertionError("expected ToolError")
    except ToolError as e:
        assert e.code == "invalid_arguments"


def test_effect_parse() -> None:
    assert Effect.parse("idempotent_write") is Effect.IDEMPOTENT_WRITE
    assert lookup.effect is Effect.READ_ONLY


def test_idempotent_contradiction() -> None:
    try:

        @tool(effect="NON_IDEMPOTENT_WRITE", idempotent=True)
        def bad(x: str) -> str:
            return x

        raise AssertionError("expected ConfigurationError")
    except ConfigurationError:
        pass


def test_required_evidence_contract() -> None:
    @tool(
        effect="NON_IDEMPOTENT_WRITE",
        risk="high",
        required_evidence=["policy:POL-1", "order:{order_id}"],
        required_invariants=["coverage"],
        on_missing_evidence="block",
    )
    def payout(order_id: str) -> dict:
        return {"ok": True}

    spec = payout.to_spec()
    assert spec["revalidation_policy"] == "STRICT_CONTRACT"
    assert spec["evidence_coverage"]["required"][0]["resource_id"] == "policy:POL-1"
    assert spec["evidence_coverage"]["required"][1]["resource_id"] == "order:{order_id}"
    assert spec["evidence_coverage"]["on_missing"] == "block"


def test_direct_call_still_works() -> None:
    assert lookup("A-1")["order_id"] == "A-1"


def test_verifier_waiting_payload() -> None:
    r = VerifierResult.waiting("pending", method="order.status")
    d = r.to_dict()
    assert d["wait"] is True
    assert d["poll_after_ms"] == 500
    assert VerifierResult.verified().to_dict().get("wait") is None
