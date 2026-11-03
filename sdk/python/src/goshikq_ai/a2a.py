"""A2A protocol 1.0 JSON-RPC client (untrusted remote messages).

Remote ``completed`` / SUCCESS is not Goshikq parent-goal verification.
"""

from __future__ import annotations

from typing import Any

import httpx

from .errors import RuntimeConnectionError, RuntimeRejectedError

A2A_PROTOCOL_VERSION = "1.0"


class A2aClient:
    def __init__(self, endpoint: str, *, api_key: str | None = None, timeout: float = 30.0) -> None:
        self.endpoint = endpoint.rstrip("/")
        headers = {"content-type": "application/json"}
        if api_key:
            headers["authorization"] = f"Bearer {api_key}"
        self._client = httpx.AsyncClient(headers=headers, timeout=timeout)
        self._rpc_id = 0

    async def aclose(self) -> None:
        await self._client.aclose()

    async def agent_card(self) -> dict[str, Any]:
        url = self.endpoint
        if url.endswith("/a2a"):
            url = url[: -len("/a2a")]
        try:
            resp = await self._client.get(f"{url}/.well-known/agent-card.json")
        except httpx.HTTPError as e:
            raise RuntimeConnectionError(f"cannot fetch A2A agent card: {e}") from e
        if resp.status_code >= 400:
            raise RuntimeRejectedError("a2a_card", resp.text[:200], resp.status_code, None)
        return resp.json()  # type: ignore[no-any-return]

    async def send_message(self, text: str, *, context_id: str | None = None, idempotency_key: str | None = None) -> dict[str, Any]:
        params: dict[str, Any] = {
            "message": {
                "role": "user",
                "parts": [{"kind": "text", "text": text}],
            }
        }
        if context_id:
            params["contextId"] = context_id
            params["message"]["contextId"] = context_id
        if idempotency_key:
            params["idempotencyKey"] = idempotency_key
        return await self._rpc("SendMessage", params)

    async def get_task(self, task_id: str) -> dict[str, Any]:
        return await self._rpc("GetTask", {"id": task_id})

    async def cancel_task(self, task_id: str) -> dict[str, Any]:
        return await self._rpc("CancelTask", {"id": task_id})

    async def _rpc(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        self._rpc_id += 1
        payload = {"jsonrpc": "2.0", "id": self._rpc_id, "method": method, "params": params}
        url = self.endpoint if self.endpoint.endswith("/a2a") else f"{self.endpoint}/a2a"
        try:
            resp = await self._client.post(url, json=payload)
        except httpx.HTTPError as e:
            raise RuntimeConnectionError(f"cannot reach A2A peer at {url}: {e}") from e
        if resp.status_code >= 400:
            raise RuntimeRejectedError("a2a_http", resp.text[:200], resp.status_code, None)
        body = resp.json()
        if "error" in body:
            err = body["error"]
            raise RuntimeRejectedError(str(err.get("code", "a2a_error")), str(err.get("message", "A2A error")), resp.status_code, err.get("data"))
        result = body.get("result")
        if not isinstance(result, dict):
            raise RuntimeRejectedError("a2a_error", "A2A result was not an object", resp.status_code, body)
        return result
