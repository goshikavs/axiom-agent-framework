# Executor boundaries

## Native Rust executor (`goshikq-runtime`)

Owns:

- Run state machine and fenced leases
- Model/tool/snapshot/verify long-polls to the SDK worker
- Action journal and crash barriers
- Approval wait and external-state wait
- Persisting events, checkpoints, evidence records, decisions
- Calling the verification kernel **before** a sensitive write

Does not own:

- Python tool implementations, model clients, or in-memory demo stores
- Inferring what an LLM “internally considered”
- Distributed consensus across external business APIs

## Optional Temporal (not implemented)

Could own: timers, multi-process activity retry, cluster membership.

Must not own: goal truth, evidence invalidation, or Studio projections.

## Verification kernel (`goshikq-core`)

Owns, IO-free:

- Transition legality
- Effect-category retry rules
- Recovery policy (`recovery.rs`)
- Evidence dependency graph + deterministic change-impact (`impact.rs`)
- Redaction helpers

The kernel returns *classifications* (valid / invalid / inconclusive / requires verification). The executor applies them to rows and events.

## Reusable modules

- `state_snapshots` — already versioned observations; evidence records add resource-level identity on top
- `verification_results` / `goal_evaluations` — keep; revalidation appends new rows
- `approvals` — add `invalidated` status; do not replace the table
- `tool_invocations.dispatch_state` vs `business_operation_status` vs `recovery_disposition` — three columns, not one

## Not duplicated

Snapshots are not copied into evidence blobs. Evidence stores fingerprint + version + source. Full state remains in `state_snapshots` (redacted on read).
