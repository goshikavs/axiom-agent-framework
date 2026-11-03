# Durable recovery

Goshikq v0.2 reconstructs supported runs after a process crash. It does **not** claim exactly-once execution of arbitrary external APIs.

## What is restored

A versioned checkpoint (`schema_version = 2`) stores:

- run id, workflow id, agent version id, agent contract fingerprint, goal predicate
- phase, step, step count, limits, absolute `deadline_at` and `wait_deadline_at`
- pending invocation / approval ids and external operation reference
- last committed event sequence (column + JSON; taken from `runs.next_sequence`)
- last verified evidence version / snapshot id
- recovery generation, checkpoint schema version, creation timestamp
- pending model tool calls (so a crash before dispatch does not skip the proposed write)
- captured messages when `GOSHIKQ_CAPTURE_MODEL_IO` allows it

Python stacks, sockets, and model clients are not serialized. The worker reconnects with the same `run_id` and answers new long-polls.

Checkpoint inserts run in SQL against the current `runs` row so `last_event_sequence` and `recovery_generation` match the committed event counter. Status transitions persist events in the same database transaction as the new run status. Journal appends follow dispatch-state updates.

## Supervisor

On `goshikq-server` start:

1. Active (non-wait) native runs are marked `RECOVERY_REQUIRED`. In-flight `RUNNING` tool rows become `DISPATCH_UNCERTAIN`.
2. Durable waits (`WAITING_FOR_APPROVAL`, `WAITING_FOR_EXTERNAL_STATE`, `PAUSED`) are left as-is.
3. The supervisor claims each recoverable run with a fencing token (`runs.lease_generation`) and spawns an executor.
4. The executor reconstructs the checkpoint and applies the journal policy in [`recovery.rs`](https://github.com/goshikavs/goshikq-agent-framework/blob/main/crates/goshikq-core/src/recovery.rs).
5. Automatic resume of non-wait recoveries stops after `GOSHIKQ_MAX_RECOVERY_ATTEMPTS` (default 8) successful `resumed` attempts; durable waits are not counted against that budget.

Two workers cannot commit as the same owner: heartbeats and fenced `transition_run_leased` reject a stale generation. A lease does **not** stop an already-running old process from having sent an HTTP request; the action journal is the additional safeguard.

## Compatibility

Restore is refused (run stays `RECOVERY_REQUIRED`) when:

- the stored `CreateRunRequest` cannot be deserialized
- the agent name no longer matches
- the checkpoint `agent_config_hash` (sorted tool name/effect/approval + goal predicate) differs from the stored definition
- the checkpoint `schema_version` is newer than this binary supports

Missing state is not guessed.

## See also

- [action-journal.md](action-journal.md)
- [worker-leases.md](worker-leases.md)
- [guides/crash-recovery.md](../guides/crash-recovery.md)
