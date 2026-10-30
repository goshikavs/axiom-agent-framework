# Implementation plan

Semantic version: **0.1.0-alpha**.

| Phase | Intent | This repository |
| --- | --- | --- |
| 0 Bootstrap | Workspaces, runtime, Postgres, Studio, SDKs, CI | **In progress / largely present** |
| 1 Native agent | Mock model, tools, goals, events, one example | **Vertical slice implemented** |
| 2 Studio | Dashboard, executions, graph, timeline, live SSE, verification | **Implemented for persisted data** |
| 3 Reliability | Approvals, reconciliation, checkpoints, fault injection | **Partial** (rules + demo faults; auto-resume not implemented) |
| 4 Providers | OpenAI, Anthropic, TS SDK, MCP | **Experimental adapters**; TS SDK inspect/submit only; MCP planned |
| 5 External frameworks | LangGraph, OTel, … | **Planned** (ingestion API exists) |
| 6 Advanced Studio | Replay, compare, workflow exec, evals | **Partial UI**; workflow execution planned |
| 7 Open-source beta | Docs site, packaging, security review | **Not claimed** |

Next exact steps:

1. Keep embedded PostgreSQL working on Windows ARM64 (x64 emulation) and Linux CI.
2. Add a Python worker loop to the TypeScript SDK or document it as Python-only for execution.
3. Wire `POST /api/v1/runs/{id}/resume` for `RECOVERY_REQUIRED`.
4. MCP adapter using the same `ToolSpec` permission path.
5. Generic HTTP/OTel event mapper with capability manifests.
6. Playwright coverage of Studio live updates.
7. Measure benchmarks; do not invent numbers.
