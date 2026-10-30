"""Repeatable framework-overhead bench (health + workflow echo). Does not include model latency."""

from __future__ import annotations

import json
import os
import statistics
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import httpx

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = os.environ.get("AXIOM_RUNTIME_URL", "http://127.0.0.1:7400").rstrip("/")
OUT = ROOT / "benchmarks" / "v0.9" / "results.json"


def pct(xs: list[float], p: float) -> float:
    if not xs:
        return 0.0
    ys = sorted(xs)
    k = min(len(ys) - 1, max(0, int(round((p / 100) * (len(ys) - 1)))))
    return ys[k]


def health_latencies(client: httpx.Client, n: int) -> list[float]:
    out = []
    for _ in range(n):
        t0 = time.perf_counter()
        client.get("/health").raise_for_status()
        out.append((time.perf_counter() - t0) * 1000)
    return out


def echo_workflow(client: httpx.Client) -> float:
    name = f"bench-{uuid.uuid4().hex[:8]}"
    definition = {
        "name": name,
        "version": 1,
        "nodes": [
            {"id": "start", "kind": "start", "label": "s", "config": {}},
            {"id": "echo", "kind": "tool", "label": "echo", "config": {"tool": "echo", "handler": "echo"}},
            {"id": "end", "kind": "end", "label": "e", "config": {}},
        ],
        "edges": [{"id": "e1", "source": "start", "target": "echo"}, {"id": "e2", "source": "echo", "target": "end"}],
        "max_steps": 8,
    }
    t0 = time.perf_counter()
    created = client.post("/api/v1/workflows", json={"definition": definition, "draft": True})
    created.raise_for_status()
    wf_id = created.json()["id"]
    client.post(f"/api/v1/workflows/{wf_id}/publish").raise_for_status()
    started = client.post(f"/api/v1/workflows/{wf_id}/run", json={"input": {"n": 1}})
    started.raise_for_status()
    run_id = started.json()["run_id"]
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        st = client.get(f"/api/v1/runs/{run_id}").json()["status"]
        if st in {"COMPLETED", "FAILED", "UNVERIFIED", "TIMED_OUT"}:
            break
        time.sleep(0.05)
    return (time.perf_counter() - t0) * 1000


def main() -> None:
    levels = [int(x) for x in os.environ.get("AXIOM_BENCH_CONCURRENCY", "1,10").split(",") if x]
    results: dict = {
        "hardware": {
            "os": os.name,
            "cpu_count": os.cpu_count(),
        },
        "notes": "Health probes measure framework HTTP overhead only. Echo workflows use native handlers (no model).",
        "levels": {},
    }
    with httpx.Client(base_url=RUNTIME, timeout=30) as c:
        c.get("/health").raise_for_status()
        health = health_latencies(c, 50)
        results["health_ms"] = {
            "n": len(health),
            "p50": pct(health, 50),
            "p95": pct(health, 95),
            "p99": pct(health, 99),
            "mean": statistics.fmean(health),
        }
        for conc in levels:
            samples: list[float] = []
            errors = 0
            t0 = time.perf_counter()
            with ThreadPoolExecutor(max_workers=conc) as pool:
                futs = [pool.submit(echo_workflow, c) for _ in range(conc)]
                for f in futs:
                    try:
                        samples.append(f.result())
                    except Exception as e:
                        errors += 1
                        results.setdefault("errors_sample", []).append(str(e)[:300])
            elapsed = time.perf_counter() - t0
            results["levels"][str(conc)] = {
                "concurrency": conc,
                "ok": len(samples),
                "errors": errors,
                "wall_s": elapsed,
                "throughput_per_s": (len(samples) / elapsed) if elapsed else 0,
                "p50_ms": pct(samples, 50),
                "p95_ms": pct(samples, 95),
                "p99_ms": pct(samples, 99),
            }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
