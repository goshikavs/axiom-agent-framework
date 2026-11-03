"""Standalone A2A 1.0 JSON-RPC peer. Not an Goshikq runtime.

Completes tasks in-process. Its SUCCESS is an untrusted remote assertion.
"""

from __future__ import annotations

import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

PROTOCOL = "1.0"
TASKS: dict[str, dict] = {}
FAIL_NEXT = False


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args) -> None:  # noqa: A003
        sys.stderr.write("a2a-peer: " + (fmt % args) + "\n")

    def _send(self, code: int, body: dict | list) -> None:
        raw = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self) -> None:  # noqa: N802
        if urlparse(self.path).path == "/.well-known/agent-card.json":
            host = self.headers.get("host", "127.0.0.1")
            self._send(
                200,
                {
                    "name": "standalone-a2a-peer",
                    "description": "Independent A2A agent. Completion is not Goshikq verification.",
                    "version": "0.7.0",
                    "protocolVersion": PROTOCOL,
                    "capabilities": {"streaming": False, "pushNotifications": False},
                    "defaultInputModes": ["text/plain"],
                    "defaultOutputModes": ["application/json"],
                    "skills": [{"id": "echo", "name": "Echo", "description": "Echo or fail", "tags": ["demo"]}],
                    "supportedInterfaces": [
                        {"url": f"http://{host}/a2a", "protocolBinding": "JSONRPC", "protocolVersion": PROTOCOL}
                    ],
                },
            )
            return
        self._send(404, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802
        global FAIL_NEXT
        if urlparse(self.path).path != "/a2a":
            self._send(404, {"error": "not found"})
            return
        length = int(self.headers.get("content-length") or 0)
        payload = json.loads(self.rfile.read(length) or b"{}")
        method = payload.get("method")
        params = payload.get("params") or {}
        rpc_id = payload.get("id")
        if method in ("SendMessage", "message/send"):
            text = ""
            msg = params.get("message") or params
            for part in msg.get("parts") or []:
                if isinstance(part, dict) and part.get("text"):
                    text += str(part["text"])
            task_id = f"peer-{len(TASKS) + 1}"
            fail = FAIL_NEXT or "FAIL" in text.upper()
            FAIL_NEXT = False
            state = "failed" if fail else "completed"
            TASKS[task_id] = {
                "id": task_id,
                "contextId": params.get("contextId") or task_id,
                "status": {"state": state, "message": {"role": "agent", "parts": [{"kind": "text", "text": text or state}]}},
                "metadata": {"peer": True, "untrusted": True, "assertion": "SUCCESS" if not fail else "FAILURE"},
            }
            self._send(200, {"jsonrpc": "2.0", "id": rpc_id, "result": TASKS[task_id]})
            return
        if method in ("GetTask", "tasks/get"):
            tid = str((params.get("id") or params.get("taskId") or ""))
            task = TASKS.get(tid)
            if not task:
                self._send(200, {"jsonrpc": "2.0", "id": rpc_id, "error": {"code": -32001, "message": f"unknown task {tid}"}})
                return
            self._send(200, {"jsonrpc": "2.0", "id": rpc_id, "result": task})
            return
        if method in ("CancelTask", "tasks/cancel"):
            tid = str((params.get("id") or ""))
            task = TASKS.get(tid)
            if task:
                task["status"]["state"] = "canceled"
            self._send(200, {"jsonrpc": "2.0", "id": rpc_id, "result": task or {"id": tid, "status": {"state": "canceled"}}})
            return
        self._send(200, {"jsonrpc": "2.0", "id": rpc_id, "error": {"code": -32601, "message": f"unsupported {method}"}})


def main() -> None:
    port = int(sys.argv[1] if len(sys.argv) > 1 else 8765)
    httpd = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"a2a-peer listening on 127.0.0.1:{port}", flush=True)
    httpd.serve_forever()


if __name__ == "__main__":
    main()
