# Test report

This file records **executed** tests only. Counts come from command output, not from earlier chat claims.

## How to re-run

```powershell
# Rust (this Windows host needs the x64 MSVC env)
scripts/with-msvc.ps1 cargo +stable-x86_64-pc-windows-msvc test --workspace --all-targets

# Independent evidence baselines (small smoke)
scripts/with-msvc.ps1 cargo +stable-x86_64-pc-windows-msvc run -p axiom-bench -- --sizes 10,100 --trials 3 --seed 42 --out benchmarks/evidence/results-smoke.json

# Full v0.3.1 bench
scripts/with-msvc.ps1 cargo +stable-x86_64-pc-windows-msvc run -p axiom-bench --release -- --sizes 10,100,1000,10000 --trials 10 --seed 42 --out benchmarks/evidence/results.json

# Python SDK
python -m pytest sdk/python/tests examples/retail-refund/test_store.py -q

# Examples (separate invocations: each example exposes `main`)
python -m pytest examples/hello-agent/test_hello.py -q
python -m pytest examples/tool-agent/test_tool_agent.py -q

# Evidence + crash (needs axiom-server + DATABASE_URL)
# $env:DATABASE_URL = "postgres://postgres:axiom@127.0.0.1:55432/axiom"
# $env:AXIOM_SERVER_BIN = "...\target\debug\axiom-server.exe"
python -m pytest tests/evidence/test_evidence_scenarios.py tests/crash/test_crash_boundaries.py -v

# Studio
cd apps/studio; bun run typecheck
```

## Last executed on this host (v0.4 cycle, 2026-10-29 UTC)

| Suite | Result | Notes |
|---|---|---|
| `cargo test --workspace --all-targets` | **91 passed**, 0 failed | axiom-bench 10, axiom-core 66, axiom-protocol 15 |
| Python SDK + retail store | **34 passed** | includes OTLP JSON helper tests |
| hello-agent | **1 passed** | separate pytest invocation |
| tool-agent | **2 passed** | separate pytest invocation |
| `axiom-bench` v0.3.2 labeled metrics | previously written | `benchmarks/evidence/results-v0.3.2.json` |
| Live OTLP / LangGraph ingest | not run this cycle | needs axiom-server + optional langgraph |
| Studio `bun run typecheck` | exit 0 | coverage + OTLP inspector fields |
| evidence + crash integration | not re-run this cycle | last v0.3: 11 passed in 44.81s |

Details: `docs/milestones/v0.4-report.md` and `docs/milestones/v0.3.2-report.md`. Do not copy numbers from chat; this table is from the commands above.
