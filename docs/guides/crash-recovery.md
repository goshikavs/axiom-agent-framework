# Crash recovery

## Guarantees

- Persisted run/event/approval/checkpoint rows survive `goshikq-server` exit.
- A restarted server reattaches to `WAITING_FOR_APPROVAL` and `WAITING_FOR_EXTERNAL_STATE`.
- Uncertain non-idempotent writes are not blindly repeated.
- Exactly-once external I/O is **not** claimed.

## Commands

```powershell
# Runtime (Windows ARM64 / MSVC x64)
$env:DATABASE_URL = "postgres://postgres:goshikq@127.0.0.1:55432/goshikq"
scripts/with-msvc.ps1 cargo +stable-x86_64-pc-windows-msvc build --bin goshikq-server
.\target\debug\goshikq-server.exe

# Policy tests (no subprocess)
python -m pytest sdk/python/tests/test_recovery.py -q

# Process-crash harness (needs PostgreSQL + goshikq-server binary)
$env:DATABASE_URL = "postgres://postgres:goshikq@127.0.0.1:55432/goshikq"
python -m pytest tests/crash/test_crash_boundaries.py -v
# Re-run 2026-10-28: 8 passed in 37.22s (Windows ARM64 host, x86_64 MSVC binary).
```

Barriers (`GOSHIKQ_CRASH_AT`, process exit 97). Read-only tools are skipped so the abort hits the write:

1. `before_dispatch` — after the model proposed the write, before the invocation row
2. `after_intent` — `PREPARED` persisted, worker not asked
3. `during_tool` — request may have reached the provider; local response not recorded
4. `after_accept` — provider ack persisted; wait/verify not yet recorded
5. `during_verify` — crash at the start of action verification
6. `waiting_approval` — approval row persisted; human has not decided

Also in the same file: SSE reconnect after `after_accept` (history then `?after=` / `Last-Event-ID`, no gaps or duplicate sequences) and never-settles → `UNVERIFIED` / `INCONCLUSIVE`.

## Approval after restart

1. `python examples/insurance-claims/main.py` until `WAITING_FOR_APPROVAL`
2. Stop `goshikq-server` (kill the process)
3. Start `goshikq-server` again
4. Approve in Studio `/approvals` or `POST /api/v1/approvals/{id}/decision`
5. The original payout invocation is reused; a second payout row is not created
6. Invariants are re-evaluated against a fresh snapshot before the write runs

The crash harness `test_approval_survives_restart` covers this with process exit 97.
