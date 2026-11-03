from __future__ import annotations

import importlib.util
from pathlib import Path

from goshikq_ai.errors import ToolError


def _load_example_main():
    path = Path(__file__).resolve().parent / "main.py"
    spec = importlib.util.spec_from_file_location("goshikq_example_tool_agent_main", path)
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


lookup_sku = _load_example_main().lookup_sku


def test_valid_lookup() -> None:
    assert lookup_sku("SKU-9")["found"] is True


def test_schema_rejects_int() -> None:
    try:
        lookup_sku.validate_arguments({"sku": 9})
        raise AssertionError("expected ToolError")
    except ToolError as e:
        assert e.code == "invalid_arguments"
