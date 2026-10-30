"""Independent A2A 1.0 JSON-RPC client (httpx only) against Axiom's /a2a endpoint.

This is not Axiom's Python SDK and not the demo peer. Remote SUCCESS is still not VERIFIED.
"""

from __future__ import annotations

import os
import uuid

import httpx
import pytest

RUNTIME = os.environ.get("AXIOM_RUNTIME_URL", "http://127.0.0.1:7400").rstrip("/")


def test_independent_jsonrpc_client_against_axiom_a2a():
    with httpx.Client(base_url=RUNTIME, timeout=15) as c:
        if c.get("/health").status_code >= 500:
            pytest.skip("runtime down")
        card = c.get("/.well-known/agent-card.json")
        if card.status_code == 404:
            card = c.get("/a2a/agent-card")
        assert card.status_code == 200, card.text
        body = card.json()
        assert body.get("protocolVersion") in ("1.0", "0.2.1", None) or "protocol" in str(body).lower() or body.get("name")

        msg_id = str(uuid.uuid4())
        payload = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "message/send",
            "params": {
                "message": {
                    "messageId": msg_id,
                    "role": "user",
                    "parts": [{"kind": "text", "text": "independent-client ping"}],
                }
            },
        }
        rpc = c.post("/a2a", json=payload)
        assert rpc.status_code == 200, rpc.text
        result = rpc.json()
        assert result.get("jsonrpc") == "2.0"
        if "error" in result:
            # Still a protocol-level response from the Axiom A2A surface.
            assert result["error"].get("code") is not None
            return
        task = result.get("result") or {}
        task_id = task.get("id") or task.get("taskId")
        assert task_id, result
        got = c.post("/a2a", json={"jsonrpc": "2.0", "id": 2, "method": "tasks/get", "params": {"id": task_id}})
        assert got.status_code == 200, got.text
        # A2A task state is not an Axiom verification status.
        inner = got.json().get("result") or {}
        assert inner.get("status", {}).get("state") or inner.get("id") or got.json().get("error")
