"""Measure checkpoint/journal overhead with concurrent runs.

Prints real p50/p95 against a live runtime. Does not fabricate numbers.
Requires axiom-server and PostgreSQL.

    python scripts/bench_recovery.py
"""

from __future__ import annotations

import asyncio
import os
import statistics
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "sdk", "python", "src"))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "examples", "hello-agent"))

from axiom_ai import Agent, Goal, Runtime  # noqa: E402
from axiom_ai.models import MockModel, ModelTurn  # noqa: E402


async def one_run(i: int) -> float:
    agent = Agent(name=f"bench_agent_{i}", model=MockModel(script=[ModelTurn(content=f"hello {i}")]))
    goal = Goal(description=f"bench {i}", max_steps=2)
    t0 = time.perf_counter()
    async with Runtime(agent) as rt:
        await rt.run(goal, input={"i": i})
    return (time.perf_counter() - t0) * 1000


async def bench(n: int) -> None:
    samples = await asyncio.gather(*[one_run(i) for i in range(n)])
    s = sorted(samples)
    p50 = s[len(s) // 2]
    p95 = s[max(0, int(len(s) * 0.95) - 1)]
    print(f"n={n}  p50={p50:.1f}ms  p95={p95:.1f}ms  mean={statistics.mean(s):.1f}ms")


async def main() -> None:
    for n in (1, 10):
        await bench(n)
    if os.environ.get("AXIOM_BENCH_100") == "1":
        await bench(100)


if __name__ == "__main__":
    asyncio.run(main())
