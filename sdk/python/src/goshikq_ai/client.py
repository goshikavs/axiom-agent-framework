"""HTTP client for the Goshikq runtime API (v1)."""

from __future__ import annotations

from typing import Any

import httpx

from ._env import branded
from ._version import __version__
from .errors import RuntimeConnectionError, RuntimeRejectedError

DEFAULT_ENDPOINT = "http://127.0.0.1:7400"


class RuntimeClient:
    """Thin async wrapper over the REST API. All methods raise :class:`RuntimeRejectedError`
    for 4xx responses and :class:`RuntimeConnectionError` for transport/5xx failures."""

    def __init__(self, endpoint: str | None = None, api_key: str | None = None, *, actor: str | None = None, timeout: float = 60.0) -> None:
        self.endpoint = (endpoint or branded("RUNTIME_URL") or DEFAULT_ENDPOINT).rstrip("/")
        self.api_key = api_key or branded("API_KEY")
        headers = {"user-agent": f"goshikq-python/{__version__}"}
        if self.api_key:
            headers["authorization"] = f"Bearer {self.api_key}"
        if actor:
            headers["x-goshikq-actor"] = actor
        self._client = httpx.AsyncClient(base_url=self.endpoint, headers=headers, timeout=httpx.Timeout(timeout, read=timeout + 35))

    async def aclose(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> RuntimeClient:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.aclose()

    async def _request(self, method: str, path: str, **kwargs: Any) -> Any:
        try:
            resp = await self._client.request(method, path, **kwargs)
        except httpx.HTTPError as e:
            raise RuntimeConnectionError(f"cannot reach Goshikq runtime at {self.endpoint}: {e}") from e
        if resp.status_code >= 500:
            raise RuntimeConnectionError(f"runtime error {resp.status_code} on {method} {path}", status_code=resp.status_code, body=_safe_json(resp))
        if resp.status_code >= 400:
            body = _safe_json(resp) or {}
            err = body.get("error", {}) if isinstance(body, dict) else {}
            raise RuntimeRejectedError(err.get("code", "error"), err.get("message", resp.text[:200]), resp.status_code, err.get("details"))
        if resp.status_code == 204 or not resp.content:
            return None
        return resp.json()

    # ---- system ----
    async def health(self) -> dict[str, Any]:
        return await self._request("GET", "/health")  # type: ignore[no-any-return]

    async def ready(self) -> dict[str, Any]:
        return await self._request("GET", "/ready")  # type: ignore[no-any-return]

    # ---- runs ----
    async def create_run(self, body: dict[str, Any]) -> dict[str, Any]:
        return await self._request("POST", "/api/v1/runs", json=body)  # type: ignore[no-any-return]

    async def get_run(self, run_id: str) -> dict[str, Any]:
        return await self._request("GET", f"/api/v1/runs/{run_id}")  # type: ignore[no-any-return]

    async def list_runs(self, **params: Any) -> dict[str, Any]:
        return await self._request("GET", "/api/v1/runs", params={k: v for k, v in params.items() if v is not None})  # type: ignore[no-any-return]

    async def cancel_run(self, run_id: str, reason: str | None = None) -> dict[str, Any]:
        return await self._request("POST", f"/api/v1/runs/{run_id}/cancel", json={"reason": reason})  # type: ignore[no-any-return]

    async def start_run(self, run_id: str) -> dict[str, Any]:
        return await self._request("POST", f"/api/v1/runs/{run_id}/start")  # type: ignore[no-any-return]

    async def create_task(self, parent_run_id: str, body: dict[str, Any]) -> dict[str, Any]:
        return await self._request("POST", f"/api/v1/runs/{parent_run_id}/tasks", json=body)  # type: ignore[no-any-return]

    async def list_tasks(self, parent_run_id: str) -> list[Any]:
        return await self._request("GET", f"/api/v1/runs/{parent_run_id}/tasks")  # type: ignore[no-any-return]

    async def hierarchy(self, run_id: str) -> dict[str, Any]:
        return await self._request("GET", f"/api/v1/runs/{run_id}/hierarchy")  # type: ignore[no-any-return]

    async def handoff(self, run_id: str, body: dict[str, Any]) -> dict[str, Any]:
        return await self._request("POST", f"/api/v1/runs/{run_id}/handoff", json=body)  # type: ignore[no-any-return]

    async def put_shared_context(self, run_id: str, body: dict[str, Any]) -> dict[str, Any]:
        return await self._request("PUT", f"/api/v1/runs/{run_id}/shared-context", json=body)  # type: ignore[no-any-return]

    async def list_shared_context(self, run_id: str) -> list[Any]:
        return await self._request("GET", f"/api/v1/runs/{run_id}/shared-context")  # type: ignore[no-any-return]

    async def list_events(self, run_id: str, after: int | None = None, limit: int = 500) -> dict[str, Any]:
        params: dict[str, Any] = {"limit": limit}
        if after is not None:
            params["after"] = after
        return await self._request("GET", f"/api/v1/runs/{run_id}/events", params=params)  # type: ignore[no-any-return]

    async def get_verification(self, run_id: str) -> dict[str, Any]:
        return await self._request("GET", f"/api/v1/runs/{run_id}/verification")  # type: ignore[no-any-return]

    async def get_metrics(self, run_id: str) -> dict[str, Any]:
        return await self._request("GET", f"/api/v1/runs/{run_id}/metrics")  # type: ignore[no-any-return]

    async def get_state(self, run_id: str) -> dict[str, Any]:
        return await self._request("GET", f"/api/v1/runs/{run_id}/state")  # type: ignore[no-any-return]

    # ---- worker protocol ----
    async def poll(self, run_id: str, timeout_ms: int = 20_000) -> dict[str, Any]:
        return await self._request("POST", f"/api/v1/runs/{run_id}/worker/poll", params={"timeout_ms": timeout_ms})  # type: ignore[no-any-return]

    async def respond(self, run_id: str, request_id: str, *, result: Any = None, error: dict[str, Any] | None = None) -> None:
        body: dict[str, Any] = {"request_id": request_id, "ok": error is None}
        if error is None:
            body["result"] = result
        else:
            body["error"] = error
        await self._request("POST", f"/api/v1/runs/{run_id}/worker/respond", json=body)

    # ---- approvals ----
    async def list_approvals(self, status: str | None = "pending", run_id: str | None = None) -> dict[str, Any]:
        return await self._request("GET", "/api/v1/approvals", params={k: v for k, v in {"status": status, "run_id": run_id}.items() if v})  # type: ignore[no-any-return]

    async def decide_approval(self, approval_id: str, decision: str, reason: str | None = None) -> dict[str, Any]:
        return await self._request("POST", f"/api/v1/approvals/{approval_id}/decision", json={"decision": decision, "reason": reason})  # type: ignore[no-any-return]

    # ---- events ----
    async def ingest_events(self, events: list[dict[str, Any]]) -> dict[str, Any]:
        return await self._request("POST", "/api/v1/events/batch", json={"events": events})  # type: ignore[no-any-return]

    async def ingest_otlp_traces(self, payload: dict[str, Any] | bytes, *, content_type: str | None = None) -> Any:
        """POST OTLP traces. JSON dicts use application/json; bytes default to protobuf."""
        if isinstance(payload, (bytes, bytearray)):
            headers = {"content-type": content_type or "application/x-protobuf"}
            return await self._request("POST", "/v1/traces", content=payload, headers=headers)
        return await self._request("POST", "/v1/traces", json=payload)


def _safe_json(resp: httpx.Response) -> Any:
    try:
        return resp.json()
    except ValueError:
        return None
