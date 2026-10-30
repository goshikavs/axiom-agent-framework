# Pending external operations

`WAITING_FOR_EXTERNAL_STATE` is a durable wait. The runtime polls the `StateProvider` and does not call the model or re-dispatch the write.

## Delayed settlement (happy wait)

```powershell
python examples/retail-refund/main.py --fault delayed_settlement
```

Re-run 2026-10-28, run `01a185c1-c575-73c1-9e2b-54d069ffad5b`:

- `COMPLETED` / `VERIFIED`, 3 steps
- `tool_status=SUCCEEDED`, `dispatch=OUTCOME_CONFIRMED`, `business_op=SETTLED`
- output: `external operation submit_refund settled against authoritative state`
- historical action verification row remains `INCONCLUSIVE` (accepted, not yet settled). It is not rewritten.

The simulated processor accepts, then settles after ~2s of wall clock. The run stays at 3 steps (lookup, eligibility, submit). It must not fail as `MODEL_BUDGET_EXCEEDED` or `STEP_LIMIT_EXCEEDED`.

## Never settles

```powershell
python examples/retail-refund/main.py --fault never_settles
```

Re-run 2026-10-28, run `01a185c1-f6e5-7130-acc0-348a15ceab9a`:

- `UNVERIFIED` / `INCONCLUSIVE`, 3 steps (not a model or step budget failure)
- `tool_status=SUCCEEDED`, `business_op=PENDING_SETTLEMENT`
- failure: `GOAL_VERIFICATION_INCONCLUSIVE: external settlement did not complete before the wait deadline; the original write was not re-dispatched`
- `store_order_status: refund_pending`, `store_refund_status: accepted`, external id `rf-1`
- Studio recovery panel: warning, checkpoint `WAITING_FOR_EXTERNAL_STATE`, 99 unique event sequences

At `external_wait_seconds` (default 8 for this fault):

- polling stops
- verification is `INCONCLUSIVE` (`GOAL_VERIFICATION_INCONCLUSIVE`)
- run status is `UNVERIFIED`
- the refund id is preserved
- the failure reason explains that manual reconciliation is required

## Restart while waiting

Kill `axiom-server` during `WAITING_FOR_EXTERNAL_STATE`, start it again, keep the Python worker (or reconnect). The wait deadline is the original UTC instant from the checkpoint, not `now() + duration`. The existing refund is rediscovered; `submit_count` must not create a second processor record for the same idempotency key.

Covered by `tests/crash/test_crash_boundaries.py` barriers `after_accept` and `during_verify`.
