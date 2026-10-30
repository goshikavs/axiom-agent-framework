# Backup and restore

Axiom state is PostgreSQL. Files under `.axiom/postgres` are an embedded instance, not a backup format.

## Backup

```bash
pg_dump --format=custom --file axiom.dump "$DATABASE_URL"
```

Include all schemas. Migrations are additive; dump the whole database.

## Restore

```bash
pg_restore --clean --if-exists --dbname "$DATABASE_URL" axiom.dump
axiom-server --migrate-only
```

Restore onto the same major PostgreSQL version when possible. After restore, run readiness checks (`/health`, `/ready`) before sending traffic.

## What is not in the dump

- SDK worker processes
- In-memory mailboxes (workers must re-poll)
- Model provider credentials (they are environment, not rows)

Action-journal rows in an uncertain dispatch stay uncertain after restore; do not re-dispatch blindly.
