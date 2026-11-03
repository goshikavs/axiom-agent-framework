# Remaining roadmap (v0.3.2 → v1.0)

Status labels: **IMPLEMENTED_AND_TESTED**, **IMPLEMENTED_UNTESTED**, **PARTIAL**, **PLANNED**, **BLOCKED**.

Do not treat this file as a completion claim. Each milestone still needs a report under `docs/milestones/`.

## Dependency graph

```text
v0.3 (evidence + recovery) ──► v0.3.1 (independent baselines + seeded bench)
                                 │
                                 ▼
                               v0.3.2  Evidence coverage contracts + action gates
                                 │
                                 ▼
                               v0.4  OpenTelemetry + LangGraph + OpenAI Agents SDK
                                 │
                                 ▼
                               v0.5  Studio explorer / replay / compare / cost
                                 │
                                 ▼
                               v0.6  TypeScript SDK + Bun + MCP
                                 │
                                 ▼
                               v0.7  Multi-agent + A2A + optional Temporal
                                 │
                                 ▼
                               v0.8  CLI, workflow execution, docs site, packaging
                                 │
                                 ▼
                               v0.9  Authn/z, telemetry security, load, platforms
                                 │
                                 ▼
                               v1.0  Release candidate (honest scope)
```

A later milestone must not start while a prior **release gate** is failing. Optional integrations may be marked experimental and excluded from v1.0 rather than forcing unsafe stubs.

## v0.3 — evidence-aware verification

**IMPLEMENTED_AND_TESTED** (see `docs/IMPLEMENTATION_STATUS.md`, `docs/milestones` if present).

Preserved. Not reimplemented.

## v0.3.1 — research and benchmark validation

**IMPLEMENTED_AND_TESTED** — `docs/milestones/v0.3.1-report.md`.

| Item | Status |
|---|---|
| Four independent strategies A–D in `goshikq-core` | IMPLEMENTED_AND_TESTED |
| Seeded generator 10 / 100 / 1_000 / 10_000 | IMPLEMENTED_AND_TESTED (seed 42, 10 trials, 12 families) |
| Independent ground truth (true graph, hidden edges) | IMPLEMENTED_AND_TESTED |
| Machine-readable results | IMPLEMENTED_AND_TESTED (`benchmarks/evidence/results.json`, `summary.json`) |
| Reproduction doc | IMPLEMENTED_AND_TESTED (`docs/benchmarks/evidence-verification.md`) |
| End-to-end SDK latency (`tests/perf/bench_evidence.py`) | IMPLEMENTED_AND_TESTED as a **workload** bench, not as independent A/B/C/D |

Gate met: all four strategies execute; generator is deterministic for a fixed seed; ground truth is not `compute_impact`; results are JSON; no fabricated speedup claims.

## v0.3.2 — evidence coverage and safe action gating

**IMPLEMENTED_AND_TESTED** — `docs/milestones/v0.3.2-report.md`.

Labeled metrics so full-declared 100% recall is not misread as hidden-edge detection. `EvidenceCoverageContract`, IO-free coverage evaluator, risk-aware gates, policies SELECTIVE / FULL_DECLARED / SELECTIVE_PLUS_PREWRITE / STRICT_CONTRACT. Coverage-complete is not goal VERIFIED. Explicit contracts cannot prove completeness of hidden model dependencies.

## v0.4 — OpenTelemetry and framework interoperability

**IMPLEMENTED_AND_TESTED** (mapper + manifests + HTTP receiver compiled with workspace tests). Live LangGraph/OpenAI Agents runs against `goshikq-server` are **IMPLEMENTED_UNTESTED** on this host until a server session is recorded. External traces never imply verified business outcomes. gRPC OTLP is not implemented.

Depends on: v0.3.2 gate (so Studio later can show measured, not invented, verification behavior).

## v0.5 — advanced Studio

**PARTIAL.** Native explorer, graph/timeline/inspector, evidence pages, recovery panel, SSE exist. Missing: real filter/pagination/export against persisted data, synchronized three-panel selection, historical replay without side effects, run diff, failure analysis, cost analytics with unknown pricing left unknown, Playwright suite.

Depends on: v0.4 if framework source/capability columns are in scope; otherwise native-only explorer can proceed after v0.3.1.

## v0.6 — TypeScript SDK, Bun, MCP

**PARTIAL.** `sdk/typescript` has types and inspect/submit, **no worker loop**. No MCP adapter. No `examples/typescript-refund-agent`.

Depends on: existing Rust runtime (do not duplicate an execution engine).

## v0.7 — multi-agent, A2A, optional Temporal

**IMPLEMENTED_AND_TESTED** (2026-10-29). Report: `docs/milestones/v0.7-report.md`. Native `AgentGroup`, verified delegation, shared context, A2A 1.0 JSON-RPC subset, Studio hierarchy panel. Temporal adapter **deferred** (native executor remains canonical).

Depends on: v0.3 evidence/goal model remaining canonical. Temporal stays optional.

## v0.8 — developer experience

**IMPLEMENTED_AND_TESTED** with documented MCP/A2A limits. Report: `docs/milestones/v0.8-report.md`. Executable workflows (Rust executor), Studio React Flow editor, CLI `init`/`workflows`, VitePress docs site, local packaging smoke tests. Do not start v0.9 until v0.8 gates stay green.

Depends on: working runtime (v0.3) plus enough Studio (v0.5) that `goshikq studio` opens a real console.

## v0.9 — security, reliability, production hardening

**PARTIAL** (code in tree; treat as incomplete until crash + OIDC isolation + registered-tool tests are green on the rebuilt binary). Report: `docs/milestones/v0.9-report.md`.

OIDC/RBAC, membership, registered TOOL dispatch, `WORKER_UNAVAILABLE` for spawn, runtime metrics, threat model, Docker files. Not claimed: Docker smoke on this host, native Windows ARM64, MCP Streamable HTTP, OTEL runtime exporter, 1k concurrency.

Depends on: advertised surfaces from v0.4–v0.8 so security tests cover real routes.

## v1.0 — open-source release candidate

**PLANNED.** No git remote, no GitHub repo assumed. Do not commit/push/publish without owner authorization.

Gate: core execution, verification, checkpoints, sensitive API access, honest Studio, tested README quickstart, compatible SDKs, green required tests. Incomplete optional integrations are documented experimental or out of scope.

## Continuation

v0.9 gates are recorded in `docs/milestones/v0.9-report.md`. Do not start v1.0 until those suites stay green. No commit/push/publish without owner authorization.
