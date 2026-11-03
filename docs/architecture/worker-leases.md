# Worker leases

Each executor claims `runs.lease_owner` + `runs.lease_generation` (fencing token) with `lease_expires_at`.

- Startup uses `steal_run_lease` because the previous process is gone.
- Heartbeats refresh expiry (`GOSHIKQ_LEASE_TTL_SECS`, default 30; crash tests use 8).
- A rejected heartbeat cancels the local executor.
- Status transitions from a claimed executor go through `transition_run_leased`. If the generation no longer owns the row, the write is rejected (`Conflict`) and the executor stops.
- Event emit and checkpoint writes check `lease_is_valid` first.
- `GET /api/v1/runs/{id}/recovery` shows generation, owner, and journal state.

A live lease does not prove that an old worker did not already send a side effect. Combine leases with the [action journal](action-journal.md).
