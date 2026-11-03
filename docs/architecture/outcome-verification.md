# Outcome verification

Goshikq splits five independent fields. Do not treat any one as a proxy for another.

| Field | Owner | Example pending refund |
|---|---|---|
| Execution status | Run state machine | `WAITING_FOR_EXTERNAL_STATE` |
| Tool status | Tool invocation row | `SUCCEEDED` (processor accepted) |
| External business-operation status | Authoritative system via snapshot | `PENDING_SETTLEMENT` / `accepted` |
| Goal satisfaction | Success predicate on snapshot | `false` until `order.status=refunded` and `refund.status=settled` |
| Goal verification status | Verifier / predicate result | `INCONCLUSIVE` while waiting; `VERIFIED` only after settlement |

`COMPLETED` is only written when the success predicate is true against a `StateProvider` snapshot. A tool HTTP 200 / `accepted: true` is not sufficient.

Current business-operation status is projected from `tool_invocations.business_operation_status` (updated as the journal observes settlement). Past `action-verify` events are not rewritten if they still say `accepted`.

Live delayed-settlement run `01a185c1-c575-73c1-9e2b-54d069ffad5b` (2026-10-28): `GET /verification` returned `tool_status=SUCCEEDED`, `business_operation_status=SETTLED`, `verification_status=VERIFIED`, `goal_satisfied=true`. The earlier action evidence row remains `INCONCLUSIVE` (`order.status=refund_pending`, `refund.status=accepted`). Studio rendered both.

Never-settles run `01a185c1-f6e5-7130-acc0-348a15ceab9a`: `UNVERIFIED` / `INCONCLUSIVE`, `tool_status=SUCCEEDED`, `business_operation_status=PENDING_SETTLEMENT`.

## Canonical contract

`GET /api/v1/runs/{id}/verification` includes `outcome`:

```json
{
  "execution_status": "UNVERIFIED",
  "tool_status": "SUCCEEDED",
  "business_operation_status": "PENDING_SETTLEMENT",
  "goal_satisfied": false,
  "verification_status": "INCONCLUSIVE",
  "evidence_source": "simulated-orders+processor",
  "evidence_version": "...",
  "observed_at": "...",
  "verifier": "order.status+refund.status"
}
```

Evidence records source, version, observation time, and verifier identity when the snapshot/verifier supplied them.

## Python

`VerifierResult.waiting(...)` sets `status=INCONCLUSIVE`, `wait=true`, and `business_operation_status`. The runtime then enters `WAITING_FOR_EXTERNAL_STATE` instead of failing the action or burning `max_steps`.

## Studio

The verification page renders the five-way contract. Pending settlement and `INCONCLUSIVE` use amber badges. Historical replay does not execute tools.
