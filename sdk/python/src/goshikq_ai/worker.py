"""Serves runtime worker requests for one run (ADR-001)."""

from __future__ import annotations

import asyncio
import inspect
import logging
from datetime import datetime
from typing import Any

from .agent import Agent
from .client import RuntimeClient
from .errors import ModelAdapterError, RuntimeConnectionError, RuntimeRejectedError, ToolError
from .goal import Goal
from .state import SnapshotContext, StateProvider, StateSnapshot

log = logging.getLogger("goshikq_ai.worker")


class Worker:
    def __init__(self, client: RuntimeClient, run_id: str, agent: Agent, goal: Goal, state_provider: StateProvider | None, *, poll_timeout_ms: int = 20_000) -> None:
        self.client = client
        self.run_id = run_id
        self.agent = agent
        self.goal = goal
        self.state_provider = state_provider
        self.poll_timeout_ms = poll_timeout_ms
        self.requests_served = 0
        self.last_status: str | None = None

    async def serve(self) -> str:
        """Poll until the runtime detaches this worker. Returns the last run status seen."""
        consecutive_failures = 0
        while True:
            try:
                poll = await self.client.poll(self.run_id, timeout_ms=self.poll_timeout_ms)
                consecutive_failures = 0
            except RuntimeConnectionError as e:
                consecutive_failures += 1
                if consecutive_failures > 40:
                    raise
                log.warning("poll failed (%s); retrying in %ss", e, min(2**consecutive_failures, 10))
                await asyncio.sleep(min(2**consecutive_failures, 10))
                continue
            self.last_status = poll.get("run_status")
            req = poll.get("request")
            if req is not None:
                await self._handle(req)
            if poll.get("terminal"):
                return self.last_status or "UNKNOWN"

    async def _handle(self, req: dict[str, Any]) -> None:
        request_id = req["request_id"]
        kind = req.get("kind")
        payload = req.get("payload") or {}
        self.requests_served += 1
        try:
            if kind == "model.generate":
                result = await self._model_generate(payload)
            elif kind == "tool.execute":
                result = await self._tool_execute(payload)
            elif kind == "state.snapshot":
                result = await self._state_snapshot(payload)
            elif kind == "goal.evaluate":
                result = await self._goal_evaluate(payload)
            elif kind == "action.verify":
                result = await self._action_verify(payload)
            else:
                await self._respond_error(request_id, "not_implemented", f"unknown request kind {kind!r}")
                return
        except ToolError as e:
            await self._respond_error(request_id, e.code, str(e), retryable=e.retryable, details=e.details)
            return
        except ModelAdapterError as e:
            await self._respond_error(request_id, e.code, str(e), retryable=e.retryable, details=e.details)
            return
        except TimeoutError:
            # The runtime enforces its own deadline; report ours as a non-retryable error so
            # effect-category rules decide what happens next.
            await self._respond_error(request_id, "timeout", f"{kind} exceeded its local timeout", retryable=False)
            return
        except Exception as e:  # noqa: BLE001 - every failure must reach the runtime as data
            log.exception("worker request %s failed", kind)
            await self._respond_error(request_id, "worker_exception", f"{type(e).__name__}: {e}", retryable=False)
            return
        try:
            await self.client.respond(self.run_id, request_id, result=result)
        except RuntimeRejectedError as e:
            log.warning("runtime rejected response for %s: %s", request_id, e)

    async def _respond_error(self, request_id: str, code: str, message: str, *, retryable: bool = False, details: Any = None) -> None:
        try:
            await self.client.respond(self.run_id, request_id, error={"code": code, "message": message, "retryable": retryable, "details": _jsonable(details)})
        except RuntimeRejectedError as e:
            log.warning("runtime rejected error response for %s: %s", request_id, e)

    # ---- handlers ----

    async def _model_generate(self, p: dict[str, Any]) -> dict[str, Any]:
        resp = await self.agent.adapter.generate(p.get("messages") or [], p.get("tools") or [], p.get("config") or {})
        return resp.to_dict()

    async def _tool_execute(self, p: dict[str, Any]) -> dict[str, Any]:
        name = p["tool_name"]
        try:
            t = self.agent.tool(name)
        except KeyError:
            raise ToolError(f"tool `{name}` is not registered on agent `{self.agent.name}`", code="unknown_tool") from None
        output = await t.invoke(p.get("arguments") or {}, idempotency_key=p.get("idempotency_key"), timeout_ms=p.get("timeout_ms"))
        return {"output": _jsonable(output)}

    async def _snapshot(self, reason: str, step: int | None) -> StateSnapshot:
        if self.state_provider is None:
            raise ToolError("no state provider registered", code="no_state_provider")
        res = self.state_provider.snapshot(SnapshotContext(run_id=self.run_id, reason=reason, step=step))
        if inspect.isawaitable(res):
            res = await res
        if not isinstance(res, StateSnapshot):
            raise ToolError("StateProvider.snapshot must return a StateSnapshot", code="invalid_snapshot")
        return res

    async def _state_snapshot(self, p: dict[str, Any]) -> dict[str, Any]:
        snap = await self._snapshot(p.get("reason", "unspecified"), p.get("step"))
        return {"state": _jsonable(snap.state), "version": snap.version, "source": snap.source, "captured_at": snap.captured_at.isoformat()}

    async def _goal_evaluate(self, p: dict[str, Any]) -> dict[str, Any]:
        snap = StateSnapshot(state=p.get("state") or {}, version=p.get("state_version"), source="runtime-provided")
        return _jsonable(await self.goal.evaluate(snap, evaluate_success=bool(p.get("evaluate_success"))))  # type: ignore[no-any-return]

    async def _action_verify(self, p: dict[str, Any]) -> dict[str, Any]:
        t = self.agent.tool(p["tool_name"])
        snap = StateSnapshot(state=p.get("state") or {}, version=p.get("state_version"), source="runtime-provided")
        res = await t.verify(p.get("arguments") or {}, p.get("output"), snap)
        return _jsonable(res.to_dict())  # type: ignore[no-any-return]


def _jsonable(value: Any) -> Any:
    """Safe serialization: pydantic models, dataclasses, datetimes, sets."""
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, dict):
        return {str(k): _jsonable(v) for k, v in value.items()}
    if isinstance(value, (list, tuple, set, frozenset)):
        return [_jsonable(v) for v in value]
    if isinstance(value, datetime):
        return value.isoformat()
    if hasattr(value, "model_dump"):
        return _jsonable(value.model_dump())
    if hasattr(value, "__dataclass_fields__"):
        return {k: _jsonable(getattr(value, k)) for k in value.__dataclass_fields__}
    if isinstance(value, BaseException):
        return {"error": type(value).__name__, "message": str(value)}
    return repr(value)
