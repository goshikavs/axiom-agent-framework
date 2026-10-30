from main import lookup_sku
from axiom_ai.errors import ToolError


def test_valid_lookup() -> None:
    assert lookup_sku("SKU-9")["found"] is True


def test_schema_rejects_int() -> None:
    try:
        lookup_sku.validate_arguments({"sku": 9})
        raise AssertionError("expected ToolError")
    except ToolError as e:
        assert e.code == "invalid_arguments"
