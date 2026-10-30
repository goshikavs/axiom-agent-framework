"""Typed tool definitions with action contracts (effect, risk, approval, verifier)."""

from __future__ import annotations

import asyncio
import inspect
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, TypeVar, get_type_hints, overload

from pydantic import BaseModel, ConfigDict, ValidationError, create_model

from .errors import ConfigurationError, ToolError
from .state import StateSnapshot


class Effect(str, Enum):
    READ_ONLY = "READ_ONLY"
    REVERSIBLE = "REVERSIBLE"
    IDEMPOTENT_WRITE = "IDEMPOTENT_WRITE"
    NON_IDEMPOTENT_WRITE = "NON_IDEMPOTENT_WRITE"
    IRREVERSIBLE = "IRREVERSIBLE"

    @classmethod
    def parse(cls, value: Effect | str) -> Effect:
        if isinstance(value, Effect):
            return value
        return cls(value.strip().upper())


class Risk(str, Enum):
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"

    @classmethod
    def parse(cls, value: Risk | str) -> Risk:
        if isinstance(value, Risk):
            return value
        return cls(value.strip().lower())


@dataclass
class VerifierResult:
    """Outcome of an action verifier. Distinct from execution status and business-operation status.

    ``status`` is VERIFIED | FAILED | INCONCLUSIVE. Set ``wait=True`` when a write was
    accepted but settlement is pending; the runtime must not re-dispatch the tool.
    """

    status: str
    expected: Any = None
    actual: Any = None
    reason: str | None = None
    method: str | None = None
    wait: bool = False
    poll_after_ms: int | None = None
    business_operation_status: str | None = None

    @classmethod
    def verified(cls, *, expected: Any = None, actual: Any = None, reason: str | None = None, method: str | None = None) -> VerifierResult:
        return cls("VERIFIED", expected, actual, reason, method)

    @classmethod
    def failed(cls, reason: str, *, expected: Any = None, actual: Any = None, method: str | None = None) -> VerifierResult:
        return cls("FAILED", expected, actual, reason, method)

    @classmethod
    def inconclusive(cls, reason: str, *, method: str | None = None) -> VerifierResult:
        return cls("INCONCLUSIVE", None, None, reason, method)

    @classmethod
    def waiting(
        cls,
        reason: str,
        *,
        expected: Any = None,
        actual: Any = None,
        method: str | None = None,
        poll_after_ms: int = 500,
        business_operation_status: str = "PENDING_SETTLEMENT",
    ) -> VerifierResult:
        return cls(
            "INCONCLUSIVE",
            expected,
            actual,
            reason,
            method,
            wait=True,
            poll_after_ms=poll_after_ms,
            business_operation_status=business_operation_status,
        )

    def to_dict(self) -> dict[str, Any]:
        d: dict[str, Any] = {"status": self.status, "expected": self.expected, "actual": self.actual, "reason": self.reason, "method": self.method}
        if self.wait:
            d["wait"] = True
            d["poll_after_ms"] = self.poll_after_ms or 500
        if self.business_operation_status:
            d["business_operation_status"] = self.business_operation_status
        return d


Verifier = Callable[[dict[str, Any], Any, StateSnapshot], VerifierResult | Awaitable[VerifierResult]]
F = TypeVar("F", bound=Callable[..., Any])


def _schema_from_signature(fn: Callable[..., Any], name: str) -> tuple[type[BaseModel], dict[str, Any]]:
    sig = inspect.signature(fn)
    hints = get_type_hints(fn)
    fields: dict[str, Any] = {}
    for pname, param in sig.parameters.items():
        if param.kind in (inspect.Parameter.VAR_POSITIONAL, inspect.Parameter.VAR_KEYWORD):
            raise ConfigurationError(f"tool `{name}`: *args/**kwargs are not supported; declare explicit parameters")
        annotation = hints.get(pname, Any)
        default = ... if param.default is inspect.Parameter.empty else param.default
        fields[pname] = (annotation, default)
    model = create_model(f"{name}_Input", __config__=ConfigDict(extra="forbid"), **fields)  # type: ignore[call-overload]
    schema = model.model_json_schema()
    schema.pop("title", None)
    schema.setdefault("type", "object")
    schema["additionalProperties"] = False
    return model, schema


@dataclass
class Tool:
    """A registered action. Create with :func:`tool`."""

    name: str
    description: str
    fn: Callable[..., Any]
    input_model: type[BaseModel]
    input_schema: dict[str, Any]
    effect: Effect = Effect.READ_ONLY
    risk: Risk = Risk.LOW
    timeout_ms: int = 30_000
    max_attempts: int = 2
    idempotent: bool = False
    requires_approval: bool = False
    required_permissions: list[str] = field(default_factory=list)
    preconditions: list[str] = field(default_factory=list)
    verifier: Verifier | None = None
    output_schema: dict[str, Any] | None = None
    revalidation_policy: str = "SELECTIVE"
    evidence_coverage: dict[str, Any] | None = None

    def to_spec(self) -> dict[str, Any]:
        """ToolSpec as understood by the runtime (protocol 1.0)."""
        spec: dict[str, Any] = {
            "name": self.name,
            "description": self.description,
            "input_schema": self.input_schema,
            "effect": self.effect.value,
            "risk": self.risk.value,
            "timeout_ms": self.timeout_ms,
            "retry_policy": {"max_attempts": self.max_attempts},
            "idempotent": self.idempotent,
            "requires_approval": self.requires_approval,
            "required_permissions": list(self.required_permissions),
            "preconditions": list(self.preconditions),
            "has_verifier": self.verifier is not None,
            "revalidation_policy": self.revalidation_policy,
        }
        if self.output_schema is not None:
            spec["output_schema"] = self.output_schema
        if self.evidence_coverage is not None:
            spec["evidence_coverage"] = self.evidence_coverage
        return spec

    def validate_arguments(self, arguments: dict[str, Any]) -> dict[str, Any]:
        try:
            return self.input_model(**arguments).model_dump()
        except ValidationError as e:
            raise ToolError(f"invalid arguments for `{self.name}`: {e.errors()}", code="invalid_arguments") from e

    async def invoke(self, arguments: dict[str, Any], *, idempotency_key: str | None = None, timeout_ms: int | None = None) -> Any:
        """Validate and execute. Raises :class:`ToolError` for structured failures."""
        kwargs = self.validate_arguments(arguments)
        if "idempotency_key" in inspect.signature(self.fn).parameters and idempotency_key is not None:
            kwargs["idempotency_key"] = idempotency_key
        timeout = (timeout_ms or self.timeout_ms) / 1000
        if inspect.iscoroutinefunction(self.fn):
            return await asyncio.wait_for(self.fn(**kwargs), timeout=timeout)
        loop = asyncio.get_running_loop()
        return await asyncio.wait_for(loop.run_in_executor(None, lambda: self.fn(**kwargs)), timeout=timeout)

    async def verify(self, arguments: dict[str, Any], output: Any, state: StateSnapshot) -> VerifierResult:
        if self.verifier is None:
            return VerifierResult.inconclusive("no verifier registered")
        res = self.verifier(arguments, output, state)
        if inspect.isawaitable(res):
            res = await res
        return res

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        """Tools remain directly callable for unit tests."""
        return self.fn(*args, **kwargs)


@overload
def tool(fn: F) -> Tool: ...
@overload
def tool(
    *,
    name: str | None = ...,
    description: str | None = ...,
    effect: Effect | str = ...,
    risk: Risk | str = ...,
    timeout_ms: int = ...,
    max_attempts: int = ...,
    idempotent: bool = ...,
    requires_approval: bool = ...,
    permissions: list[str] | None = ...,
    preconditions: list[str] | None = ...,
    verifier: Verifier | None = ...,
    output_schema: dict[str, Any] | None = ...,
    required_evidence: list[str] | None = ...,
    required_invariants: list[str] | None = ...,
    on_missing_evidence: str = ...,
    revalidation_policy: str | None = ...,
) -> Callable[[F], Tool]: ...


def tool(
    fn: F | None = None,
    *,
    name: str | None = None,
    description: str | None = None,
    effect: Effect | str = Effect.READ_ONLY,
    risk: Risk | str = Risk.LOW,
    timeout_ms: int = 30_000,
    max_attempts: int = 2,
    idempotent: bool = False,
    requires_approval: bool = False,
    permissions: list[str] | None = None,
    preconditions: list[str] | None = None,
    verifier: Verifier | None = None,
    output_schema: dict[str, Any] | None = None,
    required_evidence: list[str] | None = None,
    required_invariants: list[str] | None = None,
    on_missing_evidence: str = "block",
    revalidation_policy: str | None = None,
) -> Tool | Callable[[F], Tool]:
    """Register a function as an Axiom tool with an explicit action contract.

    ``effect`` drives retry safety in the runtime: never mark a write as
    ``IDEMPOTENT_WRITE`` or ``idempotent=True`` unless the underlying API honors the
    idempotency key the runtime passes (accept an ``idempotency_key`` parameter to
    receive it).
    """

    def decorate(f: F) -> Tool:
        tool_name = name or f.__name__
        if not tool_name.replace("_", "").replace("-", "").isalnum():
            raise ConfigurationError(f"tool name `{tool_name}` must be alphanumeric with _ or -")
        model, schema = _schema_from_signature(_without_idempotency_key(f), tool_name)
        eff = Effect.parse(effect)
        if idempotent and eff == Effect.NON_IDEMPOTENT_WRITE:
            raise ConfigurationError(f"tool `{tool_name}`: idempotent=True contradicts effect NON_IDEMPOTENT_WRITE")
        coverage = None
        if required_evidence or required_invariants:
            coverage = {
                "required": [_required_item(item) for item in (required_evidence or [])],
                "required_invariants": list(required_invariants or []),
                "on_missing": on_missing_evidence,
                "risk": Risk.parse(risk).value,
            }
        policy = (revalidation_policy or "SELECTIVE").strip().upper()
        if coverage is not None and revalidation_policy is None:
            policy = "STRICT_CONTRACT"
        return Tool(
            name=tool_name,
            description=(description or inspect.getdoc(f) or "").strip(),
            fn=f,
            input_model=model,
            input_schema=schema,
            effect=eff,
            risk=Risk.parse(risk),
            timeout_ms=timeout_ms,
            max_attempts=max_attempts,
            idempotent=idempotent,
            requires_approval=requires_approval,
            required_permissions=list(permissions or []),
            preconditions=list(preconditions or []),
            verifier=verifier,
            output_schema=output_schema,
            revalidation_policy=policy,
            evidence_coverage=coverage,
        )

    if fn is not None:
        return decorate(fn)
    return decorate


def _required_item(item: str) -> dict[str, Any]:
    """Parse `policy:POL-1`, `order:{order_id}`, or `policy.coverage` into a RequiredEvidence object."""
    if ":" in item or "{" in item:
        return {"key": item, "resource_id": item, "require_version": True, "min_trust": "authoritative"}
    family, _, rest = item.partition(".")
    out: dict[str, Any] = {"key": item, "resource_family": family or None, "require_version": True, "min_trust": "authoritative"}
    if rest in {"authority", "authorization", "approval"}:
        out["authorization"] = True
    if rest:
        out["verification_method"] = rest
    return out


def _without_idempotency_key(f: Callable[..., Any]) -> Callable[..., Any]:
    """The runtime-supplied ``idempotency_key`` must not appear in the model-facing schema."""
    sig = inspect.signature(f)
    if "idempotency_key" not in sig.parameters:
        return f
    params = [p for n, p in sig.parameters.items() if n != "idempotency_key"]

    def shadow(*args: Any, **kwargs: Any) -> Any:  # pragma: no cover - schema only
        return f(*args, **kwargs)

    shadow.__signature__ = sig.replace(parameters=params)  # type: ignore[attr-defined]
    shadow.__annotations__ = {k: v for k, v in getattr(f, "__annotations__", {}).items() if k != "idempotency_key"}
    shadow.__name__ = f.__name__
    shadow.__doc__ = f.__doc__
    return shadow
