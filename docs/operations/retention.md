# Retention and administrative cleanup

Automatic background purging of execution history is **deferred**. Databases grow with runs, events, evidence, and outbox rows.

Default advertised window: `GOSHIKQ_RETENTION_DAYS=30`. This is a policy hint, not a sweeper.

## What is stored

- `runs` (goal, status, tokens, cost)
- `execution_events` (canonical protocol events; CASCADE on run delete)
- evidence, approvals, tool invocations, journal, checkpoints
- `security_events`

Active, paused, recovering, and parent-of-active runs are never deleted by the admin purge.

## Tested cleanup

Owner-only:

```http
POST /api/v1/admin/retention/purge
{
  "dry_run": true,
  "older_than_days": 30,
  "only_run_ids": ["optional-uuid"]
}
```

`dry_run` defaults to **true**. Set `false` to DELETE matching terminal rows (`COMPLETED`, `FAILED`, `CANCELLED`, `UNVERIFIED`, timeouts/budget). PostgreSQL `ON DELETE CASCADE` removes child evidence for those runs.

Tests: `tests/security/test_retention_purge.py`.

This is not SOC 2 / HIPAA / PCI retention. Operators must snapshot PostgreSQL before destructive purge (`docs/deployment/backup-restore.md`).
