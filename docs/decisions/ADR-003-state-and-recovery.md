# ADR-003: Durable state, checkpoints, and recovery guarantees

- Status: Accepted
- Date: 2026-10-27

## Context

Runs call external systems whose side effects may be ambiguous (timeouts after a write).
The runtime must persist enough to resume, must never silently retry a non-idempotent
write, and must not promise exactly-once execution of arbitrary external APIs.

## Decision

### Storage

PostgreSQL via SQLx with versioned migrations in `crates/goshikq-storage/migrations`.
All run-scoped tables carry `project_id` for tenant scoping; every API query filters by
the caller's project. Local development seeds one organization/project (`default`).

### What is persisted, when

| Moment | Rows written (one transaction) |
| --- | --- |
| Run created | `runs`, `goals`, `agent_versions` (if definition hash is new), `execution_events` (`run.created`) |
| Each state transition | `runs.status`, `execution_events` |
| Each worker request/response | `execution_events` (+ `tool_invocations`, `model_usage`, `cost_records`, `state_snapshots`, `goal_evaluations`, `verification_results` as applicable) |
| End of each step | `run_steps`, `checkpoints` (conversation history, step counter, token totals, last state version) |
| Approval requested / decided | `approvals`, `execution_events`, `audit_logs` |

### Retry safety by effect category

| Effect | Retry on transport error | Retry on timeout |
| --- | --- | --- |
| `READ_ONLY` | yes | yes |
| `REVERSIBLE` | yes | yes |
| `IDEMPOTENT_WRITE` | yes, same idempotency key | yes, same key |
| `NON_IDEMPOTENT_WRITE` | only if the error proves the call never reached the system (`retryable=true` from the tool, e.g. connection refused) | **no** → reconcile |
| `IRREVERSIBLE` | never | **no** → reconcile |

"Reconcile" means: if the action declares an outcome verifier, the runtime takes a fresh
state snapshot and asks the verifier whether the effect happened. `verified` → treated as
succeeded; `failed` → treated as not executed and reported to the model; `inconclusive` or
no verifier → the invocation becomes `RECONCILIATION_REQUIRED` and the run enters
`RECOVERY_REQUIRED` for human handling. The runtime never guesses.

### Recovery guarantees (v0.1)

Supported:

- Every persisted event and checkpoint survives a runtime crash.
- On startup, runs left in a non-terminal state are moved to `RECOVERY_REQUIRED` with
  reason `runtime_restarted`; nothing is re-executed automatically.
- A run in `PAUSED` (approval pending) resumes when the approval decision arrives,
  including after a runtime restart, because the approval and checkpoint are durable and
  the worker re-attaches by polling.
- Approval decisions are idempotent (second decision on the same approval returns the
  recorded decision; expired approvals are rejected).

Not supported yet (documented, not implied):

- Automatic resume of `RECOVERY_REQUIRED` runs. `POST /api/v1/runs/{id}/resume` is
  planned; it will reload the last checkpoint and require operator confirmation of any
  `RECONCILIATION_REQUIRED` invocation.
- Exactly-once execution of external APIs. Idempotency keys are passed to tools; the
  tool/API must honor them.
- Cross-service atomicity. State versions are per resource and only detect staleness of
  that resource.

### Outbox

`event_outbox(event_id, run_id, published_at)` is written with each event. The publisher
task drains unpublished rows into the in-process broadcast used by SSE and (later) the
OpenTelemetry exporter. Publication is at-least-once; consumers deduplicate by `event_id`.

## Consequences

- Operators can always see exactly where a run stopped and what is ambiguous.
- Recovery is conservative: ambiguous side effects wait for a human or a verifier.
- The documented limits above are mirrored in `docs/IMPLEMENTATION_STATUS.md`.
