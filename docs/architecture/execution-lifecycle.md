# Execution lifecycle

The Rust runtime owns the run state machine (`goshikq-core::state_machine`). SDKs declare work; they do not advance status.

## Statuses (v0.1)

| Status | Terminal | Meaning |
|---|---|---|
| `CREATED` | no | Persisted, executor not yet planning |
| `PLANNING` | no | Model generate in flight |
| `VALIDATING_ACTION` | no | Proposed tool call being checked |
| `WAITING_FOR_APPROVAL` | no | Durable wait for a human decision |
| `EXECUTING` | no | Worker is running a tool |
| `RECONCILING_STATE` | no | Authoritative snapshot after an ambiguous write |
| `VERIFYING_GOAL` | no | Success predicate / invariants |
| `REPLANNING` | no | Goal not yet satisfied; another model step allowed |
| `PAUSED` | no | Operator pause (durable wait) |
| `RECOVERY_REQUIRED` | no* | Process died mid-flight; operator must inspect |
| `WAITING_FOR_EXTERNAL_STATE` | no | Write accepted; settlement not verified |
| `COMPLETED` | yes | Goal verified against authoritative state |
| `UNVERIFIED` | yes | Stopped without a verified goal |
| `FAILED` | yes | Invariant or unrecoverable error |
| `CANCELLED` | yes | Operator cancel |
| `TIMED_OUT` | yes | Wall-clock deadline. Conceptual name: `EXECUTION_TIMED_OUT` |
| `BUDGET_EXCEEDED` | yes | Legacy catch-all (kept so old rows load) |
| `STEP_LIMIT_EXCEEDED` | yes | `max_steps` exhausted |
| `MODEL_BUDGET_EXCEEDED` | yes | Token or cost budget exhausted |

\* `RECOVERY_REQUIRED` is not a successful terminal. The Python worker treats it as terminal so it detaches. The run is not `COMPLETED`.

## Migration

Existing rows stored `BUDGET_EXCEEDED` and `TIMED_OUT`. New runs write `STEP_LIMIT_EXCEEDED` / `MODEL_BUDGET_EXCEEDED` / `TIMED_OUT`. `parse("EXECUTION_TIMED_OUT")` aliases to `TIMED_OUT`. No breaking rename of persisted strings.

## Bounded wait for external settlement

When an action verifier returns `wait=true` (accepted, not settled):

1. The tool row stays `SUCCEEDED`. The write is **not** re-dispatched.
2. The run enters `WAITING_FOR_EXTERNAL_STATE`.
3. The executor polls `state.snapshot` + `goal.evaluate` every `poll_after_ms` (default 500ms).
4. Deadline = `min(ExecutionLimits.external_wait_seconds, remaining run deadline)` (default 30s).
5. If the success predicate becomes true → `COMPLETED` / `VERIFIED`.
6. If the wait deadline elapses → `UNVERIFIED` / `INCONCLUSIVE` (`GOAL_VERIFICATION_INCONCLUSIVE`). Checkpoint records the wait spec for resume.

Resume: `POST /api/v1/runs` with the same idempotency key, while status is `WAITING_FOR_EXTERNAL_STATE`, re-attaches an executor that continues polling. It does not call the original write again.

## Happy-path sequence

`CREATED` → `PLANNING` → `VALIDATING_ACTION` → (`WAITING_FOR_APPROVAL`) → `EXECUTING` → `RECONCILING_STATE` → `VERIFYING_GOAL` → `COMPLETED`.

Pending settlement inserts `WAITING_FOR_EXTERNAL_STATE` after a successful write instead of burning model steps.
