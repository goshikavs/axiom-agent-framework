# Feature matrix

Labels: **IMPLEMENTED_AND_TESTED**, **IMPLEMENTED_UNTESTED**, **PARTIAL**, **PLANNED**, **BLOCKED**.

Inventory date: 2026-10-30. Claims require code plus a passing test or a live run recorded in `docs/IMPLEMENTATION_STATUS.md` / `docs/milestones/`. Latest milestone: **v0.9.1**.

| Area | Feature | Status | Notes |
|---|---|---|---|
| Runtime | Agent execution (Rust) | IMPLEMENTED_AND_TESTED | `axiom-runtime` + `axiom-server` |
| Runtime | Mock model (no API key) | IMPLEMENTED_AND_TESTED | hello / tool / refund / insurance / multi-agent |
| Runtime | Goal contracts | IMPLEMENTED_AND_TESTED | independent of dispatch/tool status |
| Runtime | Tool contracts / journal | IMPLEMENTED_AND_TESTED | PREPARED…OUTCOME_CONFIRMED |
| Runtime | Checkpoints v2 | IMPLEMENTED_AND_TESTED | crash harness 8/8 |
| Runtime | Worker leases / fencing | IMPLEMENTED_AND_TESTED | |
| Runtime | Approval wait + resume | IMPLEMENTED_AND_TESTED | insurance + crash test |
| Runtime | External wait deadline | IMPLEMENTED_AND_TESTED | never_settles → INCONCLUSIVE |
| Runtime | SSE streaming | IMPLEMENTED_AND_TESTED | |
| Evidence | Versioned records + graph + impact | IMPLEMENTED_AND_TESTED | migrations 0003–0004 |
| Storage | PostgreSQL migrations additive | IMPLEMENTED_AND_TESTED | 0001–0008 (tenancy + retention index) |
| Python SDK | Agent, Goal, tool, Runtime, AgentGroup | IMPLEMENTED_AND_TESTED | |
| TypeScript SDK | Worker + AgentGroup | IMPLEMENTED_AND_TESTED | bun test 19 |
| MCP | JSON-RPC stdio NDJSON + Content-Length read fallback | IMPLEMENTED_AND_TESTED | Axiom client + inventory server; protocol negotiate 2024-11-05…2025-11-25 |
| MCP | Official `@modelcontextprotocol/sdk` 1.32.2 interop | PARTIAL | stdio initialize/list/call after NDJSON fix; Streamable HTTP/OAuth not claimed |
| OpenAI Agents SDK | TracingProcessor ingest | PARTIAL | tracing-only; Runner.run not offline |
| Studio | Execution + multi-agent panel | IMPLEMENTED_AND_TESTED | hierarchy from `run_tasks` |
| Multi-agent | Groups, delegation, handoff, bounds | IMPLEMENTED_AND_TESTED | matrix 12 + examples |
| Multi-agent | Verified delegation | IMPLEMENTED_AND_TESTED | child success ≠ parent VERIFIED |
| Shared context | OCC + history | IMPLEMENTED_AND_TESTED | 409 on version mismatch |
| A2A | 1.0 JSON-RPC subset | IMPLEMENTED_AND_TESTED | two-process example + matrix |
| Temporal | Optional adapter | PLANNED | deferred; see temporal-comparison.md |
| CLI | doctor, runs, verify, agents, init/dev/studio, workflows | IMPLEMENTED_AND_TESTED | python+typescript templates; workflow validate/publish/run |
| Workflows | Schema, validation, Rust execution, Studio editor | IMPLEMENTED_AND_TESTED | pinned published versions; native echo/result plus registered worker tools |
| Auth | OIDC JWT + API keys + RBAC | IMPLEMENTED_AND_TESTED | local RSA JWKS (not hosted Entra/Keycloak); HMAC loopback-only |
| Auth | Tenant isolation | IMPLEMENTED_AND_TESTED | membership 403, guessed IDs, SSE/A2A |
| Observability | Health/ready/runtime metrics/JSON logs | IMPLEMENTED_AND_TESTED | |
| Observability | Native OTLP export | IMPLEMENTED_AND_TESTED | JSON OTLP/HTTP; inbound ingest unchanged |
| Retention | Owner admin purge | IMPLEMENTED_AND_TESTED | automatic sweeper deferred |
| Packaging | Isolated wheel/tarball install | IMPLEMENTED_AND_TESTED | tempdirs outside repo; not published |
| Docker | Runtime + Studio images + compose smoke | IMPLEMENTED_UNTESTED | no Docker CLI on authoring host; CI job not run |
| GitHub | Public repo + Actions | PLANNED | no commits/remote; workflows exist but have not run |

Compatibility tested on this host: Windows x64 MSVC, Python 3.x, Bun, PostgreSQL on `127.0.0.1:55432`.
