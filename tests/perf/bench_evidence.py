"""End-to-end refund-run latency (real SDK + server), not the independent A/B/C/D study.

Modes A–D here are workload presets on the retail example. They are **not**
the four independent revalidation algorithms required by v0.3.1.

Independent strategies, seeded graphs, and ground truth live in
`crates/goshikq-bench` (`cargo run -p goshikq-bench`). See
`docs/benchmarks/evidence-verification.md`.

    python tests/perf/bench_evidence.py --mode A --concurrency 1 --repeats 5
"""

from __future__ import annotations

import argparse
import asyncio
import os
import statistics
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "examples" / "retail-refund"))
sys.path.insert(0, str(ROOT / "sdk" / "python" / "src"))

from goshikq_ai import Runtime  # noqa: E402
from store import Fault, Order, demo_store  # noqa: E402

from main import StoreProvider, build_agent, build_goal  # noqa: E402
import main as refund_main  # noqa: E402


def rss_mb() -> float | None:
    try:
        import psutil  # type: ignore

        return psutil.Process().memory_info().rss / (1024 * 1024)
    except Exception:
        return None


def unique_store(idx: int, fault: str) -> None:
    store = demo_store(fault)
    oid = f"ORD-BENCH-{idx}"
    store.orders[oid] = Order(
        id=oid,
        customer="Bench",
        amount_cents=4800,
        currency="USD",
        paid_at="2026-04-01T10:00:00+00:00",
        status="paid",
        eligible=True,
        version=1,
        return_status="received",
    )
    refund_main.STORE = store
    os.environ["GOSHIKQ_REFUND_ORDER"] = oid
    os.environ["GOSHIKQ_REFUND_FAULT"] = fault


async def one_run(endpoint: str, idx: int, fault: str) -> dict:
    unique_store(idx, fault)
    agent = build_agent()
    goal = build_goal()
    t0 = time.perf_counter()
    async with Runtime(agent, endpoint=endpoint, state_provider=StoreProvider()) as rt:
        result = await rt.run(goal, input={"order_id": os.environ["GOSHIKQ_REFUND_ORDER"], "bench": idx, "fault": fault})
    elapsed = time.perf_counter() - t0
    store = refund_main.STORE
    oid = os.environ["GOSHIKQ_REFUND_ORDER"]
    submitted = store.submit_count
    refunded = store.orders[oid].status == "refunded"
    stale = fault == Fault.RETURN_CHANGED
    detected = stale and submitted == 0
    incorrect = stale and submitted > 0
    false_positive = (not stale) and result.status != "COMPLETED"
    return {
        "s": elapsed,
        "status": result.status,
        "verified": result.verified,
        "submitted": submitted,
        "refunded": refunded,
        "detected": detected,
        "incorrect": incorrect,
        "false_positive": false_positive,
    }


async def bench(endpoint: str, mode: str, concurrency: int, repeats: int) -> None:
    fault = Fault.RETURN_CHANGED if mode in {"C", "D"} else Fault.NONE
    os.environ["GOSHIKQ_RUNTIME_URL"] = endpoint
    rows: list[dict] = []
    rss0 = rss_mb()
    t_all = time.perf_counter()
    for batch in range(repeats):
        part = await asyncio.gather(*[one_run(endpoint, batch * concurrency + i, fault) for i in range(concurrency)])
        rows.extend(part)
    wall = time.perf_counter() - t_all
    rss1 = rss_mb()
    lat = sorted(r["s"] for r in rows)
    p50 = lat[len(lat) // 2]
    p95 = lat[max(0, int(len(lat) * 0.95) - 1)]
    n = len(rows)
    stale = fault == Fault.RETURN_CHANGED
    stale_n = n if stale else 0
    recall = (sum(1 for r in rows if r["detected"]) / stale_n) if stale_n else None
    incorrect = sum(1 for r in rows if r["incorrect"]) / n
    fp = sum(1 for r in rows if r["false_positive"]) / n
    verified = sum(1 for r in rows if r["verified"]) / n
    correct = sum(1 for r in rows if (r["detected"] if stale else r["verified"])) / n
    print(f"mode={mode} fault={fault} concurrency={concurrency} repeats={repeats} n={n}")
    print(f"p50_s={p50:.3f} p95_s={p95:.3f} mean_s={statistics.mean(lat):.3f} batch_wall_s={wall:.3f}")
    print(f"stale_decision_recall={recall} incorrect_action_rate={incorrect:.3f} false_positive_revalidation_rate={fp:.3f}")
    print(f"goal_verified_rate={verified:.3f} correctly_handled_rate={correct:.3f} verification_workload_runs={n}")
    if rss0 is not None and rss1 is not None:
        print(f"client_rss_mb_before={rss0:.1f} client_rss_mb_after={rss1:.1f}")
    print("note: wall-clock includes DB, polling, mock model, simulated store, and framework. Not a Rust microbenchmark.")


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--endpoint", default=os.environ.get("GOSHIKQ_RUNTIME_URL", "http://127.0.0.1:7400"))
    p.add_argument("--mode", default="A", choices=["A", "B", "C", "D"])
    p.add_argument("--concurrency", type=int, default=1)
    p.add_argument("--repeats", type=int, default=5)
    args = p.parse_args()
    asyncio.run(bench(args.endpoint, args.mode, args.concurrency, args.repeats))


if __name__ == "__main__":
    main()
