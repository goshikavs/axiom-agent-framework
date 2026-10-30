"""Resolve ``provider/model`` identifiers to adapters. Model names are never hardcoded."""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from ..errors import ConfigurationError
from .base import ModelAdapter

Factory = Callable[[str], ModelAdapter]
_FACTORIES: dict[str, Factory] = {}


def register_provider(prefix: str, factory: Factory) -> None:
    """Register a factory for ``<prefix>/<model>`` identifiers (plugins, OpenAI-compatible hosts...)."""
    _FACTORIES[prefix] = factory


def _openai(model: str) -> ModelAdapter:
    from .openai import OpenAIAdapter

    return OpenAIAdapter(model)


def _anthropic(model: str) -> ModelAdapter:
    from .anthropic import AnthropicAdapter

    return AnthropicAdapter(model)


register_provider("openai", _openai)
register_provider("anthropic", _anthropic)


def resolve_model(model: str | ModelAdapter | Any) -> ModelAdapter:
    if not isinstance(model, str):
        if hasattr(model, "generate") and hasattr(model, "model_id"):
            return model  # type: ignore[no-any-return]
        raise ConfigurationError(f"model must be a `provider/model` string or a ModelAdapter, got {type(model).__name__}")
    if "/" not in model:
        raise ConfigurationError(f"model `{model}` must be provider-qualified, e.g. `openai/<model>` or `mock/deterministic`")
    provider, name = model.split("/", 1)
    if provider == "mock":
        raise ConfigurationError("pass a MockModel(...) instance for mock models so the script is explicit")
    factory = _FACTORIES.get(provider)
    if factory is None:
        raise ConfigurationError(f"no adapter registered for provider `{provider}` (known: {sorted(_FACTORIES)})")
    return factory(name)


def known_providers() -> list[str]:
    return sorted(_FACTORIES)
