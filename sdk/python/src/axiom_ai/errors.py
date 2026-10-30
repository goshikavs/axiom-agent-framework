from __future__ import annotations

from typing import Any


class AxiomError(Exception):
    """Base class for SDK errors."""


class RuntimeConnectionError(AxiomError):
    """The Axiom runtime could not be reached or returned an unexpected response."""

    def __init__(self, message: str, *, status_code: int | None = None, body: Any = None) -> None:
        super().__init__(message)
        self.status_code = status_code
        self.body = body


class RuntimeRejectedError(AxiomError):
    """The runtime rejected a request (4xx) with a structured error body."""

    def __init__(self, code: str, message: str, status_code: int, details: Any = None) -> None:
        super().__init__(f"{code}: {message}")
        self.code = code
        self.status_code = status_code
        self.details = details


class ToolError(AxiomError):
    """Raise from a tool to report a structured, non-retryable failure.

    ``retryable=True`` asserts that the operation did NOT take effect and may be retried
    with the same idempotency key. Only claim this when you know it to be true.
    """

    def __init__(self, message: str, *, code: str = "tool_error", retryable: bool = False, details: Any = None) -> None:
        super().__init__(message)
        self.code = code
        self.retryable = retryable
        self.details = details


class RetryableToolError(ToolError):
    """A tool failure that provably did not take effect (connection refused, 503 before write...)."""

    def __init__(self, message: str, *, code: str = "transient_error", details: Any = None) -> None:
        super().__init__(message, code=code, retryable=True, details=details)


class ModelAdapterError(AxiomError):
    """A provider call failed. ``retryable`` follows provider guidance (429/5xx/timeouts)."""

    def __init__(self, message: str, *, code: str = "provider_error", retryable: bool = False, details: Any = None) -> None:
        super().__init__(message)
        self.code = code
        self.retryable = retryable
        self.details = details


class ConfigurationError(AxiomError):
    """Invalid agent/tool/goal configuration detected before contacting the runtime."""
