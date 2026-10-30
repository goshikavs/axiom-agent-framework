"""Measure checkpoint / journal / recovery overhead with a live axiom-server.

Reports real p50/p95 wall-clock latency and RSS. Does not fabricate numbers.

Usage (PostgreSQL + axiom-server already running, or AXIOM_RUNTIME_URL set):

    python tests/perf/bench_recovery.py
    python tests/perf/bench_recovery.py --concurrency 10
    python tests/perf/bench_recovery.py --concurrency 100
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

from axiom_ai import Runtime  # noqa: E402
from store import Fault, demo_store  # noqa: E402

from main import StoreProvider, build_agent, build_goal  # noqa: E402


def rss_mb() -> float | None:
    try:
        import psutil  # type: ignore

        return psutil.Process().memory_info().rss / (1024 * 1024)
    except Exception:
        try:
            import resource

            usage = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
            # Linux: KB; macOS: bytes. Windows: resource may be missing.
            return usage / 1024 if usage > 10_000_000 else usage / 1024
        except Exception:
            return None


async def one_run(endpoint: str, idx: int) -> float:
    os.environ["AXIOM_REFUND_ORDER"] = "ORD-2048"
    os.environ["AXIOM_REFUND_FAULT"] = Fault.NONE
    os.environ["AXIOM_EXTERNAL_WAIT_SECONDS"] = "8"
    import main as refund_main

    refund_main.STORE = demo_store(Fault.NONE)
    agent = build_agent()
    goal = build_goal()
    t0 = time.perf_counter()
    async with Runtime(agent, endpoint=endpoint, state_provider=StoreProvider()) as rt:
        await rt.run(goal, input={"order_id": "ORD-2048", "bench": idx})
    return time.perf_counter() - t0


async def bench(endpoint: str, concurrency: int, repeats: int) -> None:
    os.environ["AXIOM_RUNTIME_URL"] = endpoint
    latencies: list[float] = []
    rss_before = rss_mb()
    t_all = time.perf_counter()
    for batch in range(repeats):
        times = await asyncio.gather(*[one_run(endpoint, batch * concurrency + i) for i in range(concurrency)])
        latencies.extend(times)
    wall = time.perf_counter() - t_all
    rss_after = rss_mb()
    latencies.sort()
    p50 = latencies[len(latencies) // 2]
    p95 = latencies[max(0, int(len(latencies) * 0.95) - 1)]
    print(f"concurrency={concurrency} repeats={repeats} n={len(latencies)}")
    print(f"p50_s={p50:.3f} p95_s={p95:.3f} mean_s={statistics.mean(latencies):.3f} max_s={latencies[-1]:.3f}")
    print(f"batch_wall_s={wall:.3f}")
    if rss_before is not None and rss_after is not None:
        print(f"client_rss_mb_before={rss_before:.1f} client_rss_mb_after={rss_after:.1f}")
    else:
        print("client_rss_mb=unavailable (install psutil for RSS)")


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--endpoint", default=os.environ.get("AXIOM_RUNTIME_URL", "http://127.0.0.1:7400"))
    p.add_argument("--concurrency", type=int, default=1)
    p.add_argument("--repeats", type=int, default=3)
    args = p.parse_args()
    asyncio.run(bench(args.endpoint, args.concurrency, args.repeats))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
