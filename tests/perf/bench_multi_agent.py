"""v0.7 overhead measurements. Isolated records; no accuracy claims."""

from __future__ import annotations

import json
import os
import time
import uuid
from pathlib import Path

import httpx
import pytest

from tests.multi_agent.test_v07_matrix import agent, goal, server  # noqa: F401  (server fixture)


def test_bench_delegation_and_shared_context(server: str, tmp_path: Path):
    samples = {"single_create_ms": [], "child_delegate_ms": [], "shared_put_ms": [], "hierarchy_ms": [], "a2a_send_ms": []}
    with httpx.Client(base_url=server, timeout=20) as c:
        for _ in range(8):
            t0 = time.perf_counter()
            parent = c.post("/api/v1/runs", json={"agent": agent("bench-coord"), "goal": goal("bench"), "defer_start": True}).json()
            samples["single_create_ms"].append((time.perf_counter() - t0) * 1000)
            pid = parent["run_id"]
            t1 = time.perf_counter()
            c.post(f"/api/v1/runs/{pid}/tasks", json={"agent": agent(f"bench-w-{uuid.uuid4().hex[:8]}"), "goal": goal("w"), "defer_start": True})
            samples["child_delegate_ms"].append((time.perf_counter() - t1) * 1000)
            t2 = time.perf_counter()
            c.put(f"/api/v1/runs/{pid}/shared-context", json={"key": "k", "value": {"n": 1}, "visibility": "team"})
            samples["shared_put_ms"].append((time.perf_counter() - t2) * 1000)
            t3 = time.perf_counter()
            c.get(f"/api/v1/runs/{pid}/hierarchy")
            samples["hierarchy_ms"].append((time.perf_counter() - t3) * 1000)
            t4 = time.perf_counter()
            c.post("/a2a", json={"jsonrpc": "2.0", "id": 1, "method": "SendMessage", "params": {"message": {"parts": [{"kind": "text", "text": "bench"}]}}})
            samples["a2a_send_ms"].append((time.perf_counter() - t4) * 1000)
    summary = {k: {"n": len(v), "mean_ms": round(sum(v) / len(v), 3), "max_ms": round(max(v), 3)} for k, v in samples.items()}
    out = ROOT_RESULTS()
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"label": "v0.7 multi-agent overhead", "summary": summary, "note": "Not a claim that multi-agent is faster or more accurate."}, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    assert summary["child_delegate_ms"]["mean_ms"] >= 0


def ROOT_RESULTS() -> Path:
    return Path(__file__).resolve().parents[2] / "tests" / "perf" / "results-v0.7-multi-agent.json"
