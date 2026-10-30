# Action journal

Every side-effecting tool invocation has a stable id and a `dispatch_state` distinct from `ToolStatus` and from goal verification.

| State | Meaning |
|---|---|
| `PREPARED` | Intent persisted; the worker has not been asked |
| `DISPATCH_UNCERTAIN` | The request may have reached the provider; the local response was not recorded |
| `ACCEPTED` | Provider acknowledged the request |
| `OUTCOME_PENDING` | Accepted; authoritative settlement is still pending |
| `OUTCOME_CONFIRMED` | Authoritative state confirms the business outcome |
| `REJECTED` | Provider rejected, or the action was cancelled/denied |
| `RECOVERY_REQUIRED` | Operator or reconciler must inspect; do not auto-retry |

Rows live in `tool_invocations` (current projection) and `action_journal` (append-only transitions). Historical event payloads are not rewritten.

## Retry rules

- `PREPARED`: the write is known not to have been dispatched. Resume may dispatch that invocation (it does not create a second row).
- `DISPATCH_UNCERTAIN` + idempotent/read: retry the **same** idempotency key.
- `DISPATCH_UNCERTAIN` + non-idempotent/irreversible: hold. Do not resubmit.
- `ACCEPTED` / `OUTCOME_PENDING`: wait for external state. Do not submit a new refund.

The simulated processor honors `idempotency_key`. Real processors without that guarantee require reconciliation or a human.

## Outcome dimensions

Keep these independent:

1. Tool execution status (`SUCCEEDED` can coexist with pending settlement)
2. Business operation status (`ACCEPTED` → `PENDING_SETTLEMENT` → `SETTLED`)
3. Goal verification status (`VERIFIED` only when the predicate passes)

`GET /api/v1/runs/{id}` now returns `tool_status`, `dispatch_state`, and `business_operation_status` from the latest write invocation so the Python SDK summary, Studio recovery panel, and verification page agree. Historical event payloads are not rewritten.
