# Recovery semantics

See [ADR-003](../decisions/ADR-003-state-and-recovery.md). This page is the v0.1 **guarantee list**. Goshikq does **not** claim exactly-once external side effects.

## What is persisted

Runs, events (monotonic `sequence` per run), tool invocations (including `idempotency_key`), approvals, checkpoints, state snapshots, verification rows, goal evaluations.

## Process restart

`mark_orphaned_runs` on startup sets `RECOVERY_REQUIRED` for native runs left in:

`CREATED`, `PLANNING`, `VALIDATING_ACTION`, `EXECUTING`, `RECONCILING_STATE`, `VERIFYING_GOAL`, `REPLANNING`.

It does **not** rewrite:

- `WAITING_FOR_APPROVAL`
- `WAITING_FOR_EXTERNAL_STATE`
- `PAUSED`
- already-terminal statuses
- already `RECOVERY_REQUIRED`

The original write is never automatically re-dispatched.

## Interruption boundaries

| # | Boundary | Non-idempotent write | Idempotent write / read |
|---|---|---|---|
| 1 | Before tool execution | Resume planning | Resume planning |
| 2 | After intent, before dispatch | Hold without dispatch | Hold without dispatch |
| 3 | During tool execution | Reconcile; do not auto-retry | Retry same idempotency key |
| 4 | After external accept, before completion recorded | Wait for external state | Wait for external state |
| 5 | During authoritative verification | Wait if the tool already succeeded | Resume planning otherwise |
| 6 | Waiting for human approval | Resume approval wait; do not recreate the row | same |

Executable copy: `crates/goshikq-core/src/recovery.rs`.

## Worker poll after restart

If no in-memory handle exists, `terminal` is `status.is_terminal()`, not `true`. Durable waits therefore keep a worker attached when an executor is resumed. A wait without an executor stays non-terminal in the database; a worker that is not serving that run should not invent tool calls.

## Approvals

The approval row is durable. A restart does not deny it, expire it, or dispatch the gated tool. Continuing the run after a crash still requires a live executor (not implemented as automatic replay of the tool). Supported guarantee: the approval survives; the side effect is not blindly repeated.

## Pending settlement resume

Checkpoint `data.wait` stores `invocation_id`, `tool_name`, `poll_after_ms`. Reusing the run idempotency key while `WAITING_FOR_EXTERNAL_STATE` spawns an executor that only polls authoritative state.

## Not claimed

- Exactly-once delivery to a payment processor
- Automatic completion of `RECOVERY_REQUIRED`
- Cross-process atomicity between Goshikq and an external API
- Replay of historical events causing tool execution (Studio replay is GET-only)
