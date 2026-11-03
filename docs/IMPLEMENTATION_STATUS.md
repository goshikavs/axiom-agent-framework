# Implementation status

Honest inventory as of 2026-10-30 UTC (**v0.9.1** release-gate closure in tree). Features are **available**, **experimental**, or **planned**. Nothing here is marked available unless it runs against persisted data or real code paths.

Latest milestone: **v0.9.1** (`docs/milestones/v0.9.1-report.md`). v1.0 is **not** ready (`docs/release/v1.0-readiness.md`). Temporal is deferred. No commit/push/publish this cycle.

## Commands re-run on this host (v0.9.1)

```text
cargo test -p goshikq-server --bin goshikq-server                            # 11 passed
python -m pytest tests/security tests/otel/test_runtime_export.py -q     # JWKS + retention + OTEL collector
python -m pytest tests/packaging tests/crash tests/multi_agent tests/isolation/test_api_key_project_isolation.py -q
# isolated Python wheel VERIFIED; crash 8/8; multi-agent 12; API-key isolation
python -m pytest sdk/python/tests -q                                     # 28 passed
cd sdk/typescript && bun test                                            # 19 passed
apps/studio playwright e2e/security.spec.ts                              # 3 passed, 0 skipped
python examples/hello-agent/main.py                                      # COMPLETED / VERIFIED 01a18eee-...
```

Docker compose smoke and GitHub Actions: **not run**. Platform matrix: `docs/release/platform-matrix.md`. v1.0: **not ready**.

## Commands re-run on this host (v0.9)

```text
cargo test -p goshikq-server --bin goshikq-server     # 4 passed (OIDC + RBAC)
cargo test --workspace --lib                      # goshikq-core 71 + protocol 15 + bench 10
python -m pytest tests/security tests/isolation tests/workflows/test_registered_tools.py tests/reliability -q
# 9 passed (OIDC tenant isolation, registered tools, spawn WORKER_UNAVAILABLE, concurrent approvals)
python -m pytest tests/workflows/test_workflow_execution.py tests/crash tests/multi_agent sdk/python/tests -q
# 51 passed
cd sdk/typescript && bun test                     # 19 passed
apps/studio playwright e2e/security.spec.ts       # 2 passed, 1 skipped
python tests/perf/bench_v09.py                    # health p99 ~2.4ms; echo workflow conc=10 p95 ~1.17s
GET /ready -> 200 ready/database ok
GET /api/v1/runtime/metrics -> 200
```

Live runtime after rebuild: `goshikq-server` `127.0.0.1:7400`, PostgreSQL `127.0.0.1:55432`. Studio `:3000`. Auth mode `none` for local suites; OIDC tests start their own process.

## Commands re-run on this host (v0.8, still valid)

```text
python -m pytest tests/multi_agent/test_v07_matrix.py -q          # 12 passed
python -m pytest tests/workflows/test_workflow_execution.py tests/isolation tests/protocol/test_a2a_independent_client.py -q
# 5 passed (parallel+approval COMPLETED/VERIFIED 01a18ea3-1957-70e4-a8d1-9a46458c5815)
python -m pytest tests/crash/test_crash_boundaries.py tests/workflows/test_workflow_recovery.py -q
# 9 passed
python -m pytest sdk/python/tests -q                              # 28 passed
cd sdk/typescript && bun test                                     # 19 passed (official MCP SDK initialize/list/call)
apps/studio playwright studio.spec.ts + workflows.spec.ts         # 13 + 4 passed
scripts/with-msvc.ps1 cargo +stable-x86_64-pc-windows-msvc test --package goshikq-core --lib
# 71 passed
cd docs/site && bun run build                                     # ok
python -m build (sdk/python) then venv import goshikq_ai            # ok
bun pm pack (sdk/typescript) then bun add tarball                 # ok
goshikq doctor / workflows validate|publish|run / init python|typescript
python examples/multi-agent-refund/main.py                        # 01a1895e-8cb0-703b-b2ac-b9950ec20d0c VERIFIED
```

Live runtime: `goshikq-server` on `127.0.0.1:7400` (PostgreSQL 18.6.1 on `127.0.0.1:55432`) and Studio at `http://127.0.0.1:3000`. No paid model credentials.

```text
python examples/hello-agent/main.py
# run 01a185d6-0bb3-716f-9bbc-899c05a98613  COMPLETED / VERIFIED  steps=1
# tool_status SUCCEEDED  dispatch ACCEPTED  business_op ACCEPTED

python examples/retail-refund/main.py
# run 01a185e4-11b0-7735-a8b9-8dcea48dfd91  COMPLETED / VERIFIED  steps=3
# tool_status SUCCEEDED  dispatch OUTCOME_CONFIRMED  business_op SETTLED
# store_order_status: refunded  store_refund_status: settled

python examples/retail-refund/main.py --fault delayed_settlement
# run 01a185e4-bd87-75b0-8cd6-4dddf58a87cc  COMPLETED / VERIFIED  steps=3
# tool_status SUCCEEDED  dispatch OUTCOME_CONFIRMED  business_op SETTLED
# output: external operation `submit_refund` settled against authoritative state

python examples/retail-refund/main.py --fault never_settles
# run 01a185e3-8f52-725b-93f6-0df06cef035e  UNVERIFIED / INCONCLUSIVE  steps=3
# tool_status SUCCEEDED  dispatch OUTCOME_PENDING  business_op PENDING_SETTLEMENT
# recovery_disposition MANUAL_RECONCILIATION_REQUIRED  uncertain_write=false
# failure: GOAL_VERIFICATION_INCONCLUSIVE: external settlement did not complete before the wait deadline
# store_order_status: refund_pending  store_refund_status: accepted  external id rf-1

python examples/retail-refund/main.py --fault return_changed
# run 01a185e3-48d3-7036-b3be-886ff4c56288  FAILED  tool CANCELLED  dispatch PREPARED
# failure: stale decision cannot authorize `submit_refund`
# store_order_status: paid  (processor not called)

python examples/insurance-claims/main.py
# approve in Studio → COMPLETED / VERIFIED  steps=3  payout_cents=15000  (run 01a185e5-f1c4-7541-b64f-a92e59473221)

# policy v4→v5 while approval pending: tests/evidence/...::test_insurance_policy_change_invalidates_approval
# payout_cents stays 0; approval.granted is kept; approval.invalidated is appended
```

Process-crash harness (real subprocess exit 97, then restart):

```text
test_refund_crash_boundary[before_dispatch] PASSED
test_refund_crash_boundary[after_intent] PASSED
test_refund_crash_boundary[during_tool] PASSED
test_refund_crash_boundary[after_accept] PASSED
test_refund_crash_boundary[during_verify] PASSED
test_approval_survives_restart PASSED
test_sse_reconnect_after_crash PASSED
test_never_settles_inconclusive PASSED
# 8 passed (re-run with v0.3 binary as part of 11 passed in 44.81s). No duplicate refunds.
```

## Live vertical slice (this host)

| Example | Result | Notes |
|---|---|---|
| `examples/hello-agent` | `COMPLETED` / `VERIFIED` | run `01a185d6-0bb3-716f-9bbc-899c05a98613` |
| `examples/retail-refund` happy path | `COMPLETED` / `VERIFIED` | `business_op=SETTLED`; processor accept is not treated as success |
| `examples/retail-refund --fault delayed_settlement` | `COMPLETED` / `VERIFIED` | 3 steps; waited without `BUDGET_EXCEEDED` |
| `examples/retail-refund --fault never_settles` | `UNVERIFIED` / `INCONCLUSIVE` | dispatch `OUTCOME_PENDING`, not `DISPATCH_UNCERTAIN`; `MANUAL_RECONCILIATION_REQUIRED` |
| `examples/retail-refund --fault return_changed` | `FAILED` | eligibility v3→v4; `submit_refund` never dispatched (`PREPARED`) |
| `examples/insurance-claims` | `COMPLETED` / `VERIFIED` | payout 15000 after approval; run `01a185e5-f1c4-7541-b64f-a92e59473221` |
| insurance policy v4→v5 | `FAILED` | stale approval invalidated; payout_cents stays 0 |

Studio `/executions/{never-settles-id}` recovery panel: Verification inconclusive / Manual reconciliation required, `OUTCOME_PENDING`, `PENDING_SETTLEMENT`, `MANUAL_RECONCILIATION_REQUIRED`, warning that dispatch is **not** uncertain, checkpoint `WAITING_FOR_EXTERNAL_STATE`, external id `rf-1`. `/executions/{return-changed-id}/evidence` showed `order:ORD-2048` v3, decision `INVALID`, revalidation 3→4. `/executions/{delayed-id}/verification` still shows outcome contract `tool_status=SUCCEEDED`, `business_operation_status=SETTLED`, `verification_status=VERIFIED` without rewriting earlier action rows.

SSE uses default `message` events (`event_type` inside JSON) plus a named `event: end` for the terminal. History is fetched first; the stream is opened with `?after=<max sequence>`. Reconnect uses the same query (browsers cannot set `Last-Event-ID`). Server also honors the `Last-Event-ID` header.

## Measured overhead (this host, 2026-10-28)

`python tests/perf/bench_recovery.py` against live `goshikq-server` + PostgreSQL on loopback. Wall-clock of the Python SDK `Runtime.run` path (checkpoint, journal, leases, events, and simulated processor I/O). Not a microbenchmark of Rust alone. 10- and 100-way runs share the demo store; numbers are latency, not isolated-order correctness.

| Concurrency | n | p50 | p95 | batch wall | `goshikq-server` RSS |
|---|---|---|---|---|---|
| 1 (5 sequential) | 5 | 0.889 s | 0.929 s | 4.470 s | ~70 MB |
| 10 | 10 | 3.210 s | 4.410 s | 5.023 s | ~70 MB |
| 100 | 100 | 26.791 s | 43.455 s | 45.926 s | 71.1 MB after |

Database and worker HTTP dominate. No premature Rust micro-optimizations. Concurrency 10/100 share one simulated store, so p95 growth is queueing and Postgres, not proof that the Rust loop is the bottleneck.

## Independent evidence baselines (v0.3.1, this host)

`crates/goshikq-bench` — four **distinct** algorithms, seeded graphs, ground truth from the true graph (declared ∪ hidden). Not `bench_evidence.py`.

Overall n=480 per strategy (sizes 10…10_000 × 12 families × 10 trials, seed 42, release). Full numbers: `docs/benchmarks/evidence-verification.md`.

| Strategy | Recall | Precision | Incorrect-action | Mean workload | Mean latency |
|---|---|---|---|---|---|
| A none | 0.167 | 1.000 | 0.833 | 0 | 0.33 µs |
| B full_declared | 1.000 | 0.249 | 0.000 | 2228 | 384 µs |
| C conventional_prewrite | 0.302 | 1.000 | 0.833 | 0.96 | 106 µs |
| D selective | 0.645 | 1.000 | 0.785 | 3.65 | 82 µs |

D does not detect hidden edges (recall below 1). B covers those node ids by rechecking everything (precision falls with size). Cost and DB overhead: unknown / N/A. RSS after: 38.3 MiB.

## SDK latency workload (v0.3, this host)

`python tests/perf/bench_evidence.py --mode A|C --concurrency 1 --repeats 5` against live `goshikq-server` + PostgreSQL. Independent `ORD-BENCH-*` rows. Wall-clock of SDK `Runtime.run`. **Not** the independent A–D study.

| Mode | Meaning | n | p50 | p95 | stale recall | incorrect action | false-positive reval | correctly handled |
|---|---|---|---|---|---|---|---|---|
| A | Happy-path refund (registered evidence, no mutation) | 5 | 0.930 s | 0.980 s | n/a | 0 | 0 | 1.000 |
| C | Return-status change; selective revalidation before write | 5 | 0.795 s | 0.933 s | 1.0 | 0 | 0 | 1.000 |

## Available

- Rust crates: `goshikq-protocol`, `goshikq-core`, `goshikq-storage`, `goshikq-runtime`, `goshikq-server`
- Run lifecycle with transition validation
- Worker protocol (long-poll)
- PostgreSQL schema + SQLx migrations (`0001`–`0006`, including `0006_workflows`)
- `DATABASE_URL=embedded` local Postgres (Theseus binaries; no Docker required)
- Docker Compose Postgres 16 for hosts that have Docker
- Python SDK: Agent, Goal, `@tool`, StateProvider, MockModel, Runtime worker; `RunResult` prints tool/dispatch/business-op fields when the API returns them
- Examples: `hello-agent`, `tool-agent`, `retail-refund`, `insurance-claims`, `langgraph-refund`, `openai-agents-refund`
- Studio: overview, URL-state execution explorer (sort/pagination), execution detail (synchronized graph/timeline/inspector), recovery panel, live SSE (reconnect/pause), verification (independent outcome fields), evidence inspector / dependency graph / change impact / revalidation / snapshot diff, historical replay, run comparison, failure analysis, approvals, agents, events, traces, state inspector, settings, integrations, models, cost page, Playwright e2e
- Versioned evidence records, explicit dependency graph, deterministic change-impact, selective revalidation, approval invalidation (`GET /api/v1/runs/{id}/evidence`)
- Independent revalidation baselines A–D + seeded `goshikq-bench` (v0.3.1)
- Evidence coverage contracts, risk-aware action gates, policies E/F (v0.3.2)
- CLI: `doctor`, `init --template python|typescript`, `run`, `studio`, `runs list|inspect`, `verify`, `agents list`, `workflows validate|publish|run`
- Executable workflows: versioned schema, validation, Rust pinned-version execution, Studio React Flow editor, node-level run status
- Documentation site: VitePress (`docs/site`)
- Local SDK packages: Python wheel/sdist `0.1.0a1`, TypeScript tarball `0.1.0-alpha` (not published)
- OpenAPI at `GET /api/v1/openapi.json`
- Event batch ingestion `POST /api/v1/events/batch` (idempotent)
- OTLP/HTTP protobuf and JSON trace ingest `POST /v1/traces` (mapping `otel-genai-v1`; spans are not verified outcomes)
- Durable checkpoints (`schema_version=2`), action journal, fencing leases, recovery supervisor
- Process-crash harness (`GOSHIKQ_CRASH_AT`, process exit 97)
- TypeScript SDK worker loop (same protocol as Python; Rust remains the executor)
- MCP stdio NDJSON (official SDK 1.32.2 initialize/list/call) plus Content-Length read fallback; allowlists and Goshikq contracts; Streamable HTTP/OAuth not claimed
- `POST/GET /api/v1/api-keys` (token shown once)

## Experimental

- OpenAI and Anthropic adapters (need extras + API keys; not used by quickstart)
- Workflow AGENT `spawn: true` child LLM runs (require a worker; native AGENT is the default)
- OpenAI Agents SDK `TracingProcessor` ingest (`openai-agents` 0.24.0); `Runner.run` not completed offline
- MCP HTTP transport against a third-party hosted server (stdio local server + official SDK 1.32.2 initialize/list/call are tested)

## Planned

- Google ADK / CrewAI / Microsoft Agent Framework exporters
- Temporal adapter (architecture review written; native executor remains canonical)
- MCP Streamable HTTP / OAuth / resources / prompts
- gRPC OTLP (HTTP/protobuf is implemented)
- LLM-based verification planner (deterministic rules only in v0.3)
- Inferring dependencies the model considered but never registered
- Studio tools/goals catalogs, evaluations, regression UI, benchmark UI
- Public GitHub release under `goshikavs/goshikq-agent-framework` (renamed from `axiom-agent-framework`; GitHub redirects the old URL)

## Blockers (this machine)

- Host is Windows ARM64. Native `aarch64-pc-windows-msvc` lacks `link.exe` for ARM64; builds use `stable-x86_64-pc-windows-msvc` via Visual Studio Build Tools (`scripts/with-msvc.ps1`).
- Docker Desktop and WSL are not installed. Local DB uses embedded PostgreSQL (`postgresql_embedded` with the `rustls` feature; first start downloads Theseus binaries from GitHub). Prefer `pg_ctl start` on `.goshikq/postgres` when `DATABASE_URL=embedded` races a previous shutdown.
- Embedded `initdb`/`pg_ctl` default timeout is 5s; Goshikq overrides it to 180s (`GOSHIKQ_EMBEDDED_PG_TIMEOUT_SECS`).
- `gh` is not authenticated; the GitHub remote is not created. Do not `gh repo create` until the owner confirms.
- Bun is installed at `%USERPROFILE%\.bun\bin` and may be missing from PATH.
- No local git commit or push has been made in this session.

## Recovery guarantees (v0.2)

See [durable-recovery.md](architecture/durable-recovery.md), [action-journal.md](architecture/action-journal.md), [crash-recovery.md](guides/crash-recovery.md). Executable policy: `crates/goshikq-core/src/recovery.rs`. Supervisor: `crates/goshikq-runtime/src/supervisor.rs`.

Supported:

- Versioned checkpoints (`schema_version=2`) with absolute wait/execution deadlines, contract fingerprint, last event sequence, and last evidence version
- Action journal (`PREPARED` … `OUTCOME_CONFIRMED` / `RECOVERY_REQUIRED`)
- Database leases + fencing tokens (`lease_generation`); leased transitions refuse a stale generation
- Startup supervisor resumes durable waits and safe `RECOVERY_REQUIRED` runs; auto-resume of non-wait recoveries is bounded (`GOSHIKQ_MAX_RECOVERY_ATTEMPTS`, default 8)
- `WAITING_FOR_APPROVAL` survives restart; approval then continues the original invocation after revalidating invariants
- `WAITING_FOR_EXTERNAL_STATE` survives restart; the original refund is not resubmitted
- Never-settles wait deadline → `UNVERIFIED` + `INCONCLUSIVE` (not a model/step budget failure)
- `GET /api/v1/runs/{id}/recovery` + Studio recovery panel
- Process-crash harness at six `GOSHIKQ_CRASH_AT` barriers (`tests/crash/test_crash_boundaries.py`) — all six passed on this host, plus approval-after-restart, SSE reconnect, and never-settles

Not supported / not claimed:

- Exactly-once external I/O
- Automatic retry of `DISPATCH_UNCERTAIN` non-idempotent writes
- Serializing Python stacks or live sockets
- Cross-service atomicity
- Temporal / message-broker durability

Status migration (additive): new terminals `STEP_LIMIT_EXCEEDED` and `MODEL_BUDGET_EXCEEDED`; `BUDGET_EXCEEDED` and `TIMED_OUT` still parse. `EXECUTION_TIMED_OUT` is an alias for `TIMED_OUT`. `GOAL_VERIFICATION_INCONCLUSIVE` is `verification_status=INCONCLUSIVE` plus a terminal `UNVERIFIED` when the wait deadline expires.

## Evidence-aware verification (v0.3)

See [evidence-verification-design.md](architecture/evidence-verification-design.md), [evidence-dependency-graph.md](architecture/evidence-dependency-graph.md), [change-impact-analysis.md](architecture/change-impact-analysis.md), [evidence-aware-agents.md](guides/evidence-aware-agents.md).

Implemented:

- Independent fields: dispatch state, business-operation status, recovery disposition, goal verification
- Wait-deadline on an accepted write: dispatch stays `OUTCOME_PENDING` / acknowledged; `recovery_disposition=MANUAL_RECONCILIATION_REQUIRED`; verification `INCONCLUSIVE`; historical journal rows are appended, not rewritten
- Evidence records (fingerprint + version, not source blobs)
- Explicit graph + deterministic impact (cycles, missing versions, deletes, inaccessible, duplicate changes)
- Selective revalidation rules (`coverage`, `eligibility`); stale approvals cannot authorize a write
- Studio evidence page; insurance POL-1 v4→v5; retail `return_changed`

Not implemented / not claimed:

- Inferring LLM-internal dependencies (v0.3.1 measures the miss rate; it does not close the gap)
- Treating an affected decision as automatically false
- Cross-service atomicity
- Temporal executor (trait only, in docs)
- LLM verification planner
- gRPC OTLP, A2A (v0.7+)
