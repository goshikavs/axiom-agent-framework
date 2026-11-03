import json
from pathlib import Path

from goshikq_ai.models import MockModel, ModelTurn
from goshikq_ai.models.mock import MockModel as MockImpl


def test_manifest_matches_mock_capabilities() -> None:
    root = Path(__file__).resolve().parents[3]
    manifest = json.loads((root / "config" / "model-adapters.json").read_text(encoding="utf-8"))
    mock = next(a for a in manifest["adapters"] if a["id"] == "mock")
    model = MockModel(script=[ModelTurn(content="x")])
    caps = model.capabilities.to_dict()
    assert caps == mock["capabilities"]
    assert mock["package"] == "goshikq_ai.models.mock"
    assert isinstance(model, MockImpl)
    assert mock["status"] == "available"


def test_openai_anthropic_are_experimental() -> None:
    root = Path(__file__).resolve().parents[3]
    manifest = json.loads((root / "config" / "model-adapters.json").read_text(encoding="utf-8"))
    by_id = {a["id"]: a for a in manifest["adapters"]}
    assert by_id["openai"]["status"] == "experimental"
    assert by_id["anthropic"]["status"] == "experimental"
    assert "google-gemini" in manifest["planned"]
