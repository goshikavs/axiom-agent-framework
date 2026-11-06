"""Seed deterministic Studio docs fixtures against a live goshikq-server.

Does not start the runtime. Fails if :7400 is unreachable.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from uuid import uuid4

ROOT = Path(__file__).resolve().parents[3]
RUNTIME = os.environ.get("GOSHIKQ_RUNTIME_URL", "http://127.0.0.1:7400").rstrip("/")


def get(path: str) -> object:
    with urllib.request.urlopen(f"{RUNTIME}{path}", timeout=10) as resp:
        return json.loads(resp.read().decode())


def post(path: str, payload: dict, content_type: str = "application/json") -> dict:
    data = json.dumps(payload).encode()
    req = urllib.request.Request(
        f"{RUNTIME}{path}",
        data=data,
        headers={"content-type": content_type},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        body = resp.read().decode()
        return json.loads(body) if body else {}


def require_runtime() -> None:
    try:
        health = get("/health")
    except urllib.error.URLError as exc:
        raise SystemExit(f"goshikq-server not reachable at {RUNTIME}: {exc}") from exc
    print(f"runtime ok: {health}")


def run_example(rel: str) -> None:
    script = ROOT / rel
    print(f"running {rel}")
    proc = subprocess.run([sys.executable, str(script)], cwd=str(script.parent), check=False)
    if proc.returncode != 0:
        raise SystemExit(f"{rel} exited {proc.returncode}")


def seed_langgraph_otlp() -> None:
    trace_id = uuid4().hex
    now = f"{time.time_ns()}"
    payload = {
        "resourceSpans": [
            {
                "resource": {
                    "attributes": [
                        {"key": "service.name", "value": {"stringValue": "docs-langgraph-lookup"}},
                        {"key": "goshikq.source_framework", "value": {"stringValue": "langgraph"}},
                    ]
                },
                "scopeSpans": [
                    {
                        "scope": {"name": "goshikq.docs-fixture", "version": "otel-genai-v1"},
                        "spans": [
                            {
                                "traceId": trace_id,
                                "spanId": uuid4().hex[:16],
                                "name": "lookup",
                                "kind": 3,
                                "startTimeUnixNano": now,
                                "endTimeUnixNano": now,
                                "status": {"code": 1},
                                "attributes": [
                                    {"key": "gen_ai.operation.name", "value": {"stringValue": "chat"}},
                                    {"key": "gen_ai.request.model", "value": {"stringValue": "docs-fixture"}},
                                    {"key": "gen_ai.agent.name", "value": {"stringValue": "docs-langgraph-lookup"}},
                                ],
                            }
                        ],
                    }
                ],
            }
        ]
    }
    post("/v1/traces", payload)
    print(f"seeded langgraph OTLP trace {trace_id}")


def seed_workflow() -> str:
    definition = json.loads((ROOT / "examples/visual-workflow/parallel-approval.json").read_text(encoding="utf-8"))
    definition["name"] = f"pw-docs-workflow-{int(time.time())}"
    created = post("/api/v1/workflows", {"definition": definition, "draft": True})
    wf_id = created["id"]
    post(f"/api/v1/workflows/{wf_id}/publish", {})
    run = post(f"/api/v1/workflows/{wf_id}/run", {"input": {"order_id": "DOCS-1"}})
    print(f"workflow {wf_id} run {run.get('run_id')}")
    return run["run_id"]


def start_claims_pending() -> subprocess.Popen:
    script = ROOT / "examples/insurance-claims/main.py"
    print("starting claims_agent (waits for approval)")
    return subprocess.Popen([sys.executable, str(script)], cwd=str(script.parent))


def wait_approvals(timeout: float = 60) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            body = get("/api/v1/approvals?status=pending")
            items = body.get("items") if isinstance(body, dict) else None
            if items:
                print(f"pending approvals: {len(items)}")
                return
        except urllib.error.URLError:
            pass
        time.sleep(1)
    print("warning: no pending approvals within timeout")


def main() -> int:
    require_runtime()
    run_example("examples/hello-agent/main.py")
    run_example("examples/multi-agent-refund/main.py")
    seed_langgraph_otlp()
    seed_workflow()
    claims = start_claims_pending()
    wait_approvals()
    print(f"claims pid {claims.pid} still waiting for Studio approval")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
