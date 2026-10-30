# Production configuration

## Required

```text
DATABASE_URL=postgres://...
AXIOM_BIND=0.0.0.0:7400
AXIOM_AUTH_MODE=oidc
AXIOM_OIDC_ISSUER=https://idp.example.com/...
AXIOM_OIDC_AUDIENCE=axiom
AXIOM_OIDC_JWKS_URL=https://idp.example.com/.../certs
AXIOM_LOG_FORMAT=json
AXIOM_CORS_ORIGINS=https://studio.example.com
AXIOM_CAPTURE_MODEL_IO=redacted
```

Never set `AXIOM_AUTH_MODE=none` on a non-loopback address without `AXIOM_ALLOW_INSECURE_BIND` (unsafe).

## Migrations

```bash
axiom-server --migrate-only
# or start the server; it migrates on boot
```

Additive SQL lives in `crates/axiom-storage/migrations/`. v0.9 adds `0007_tenancy.sql`.

## Readiness

- `GET /health` — process up
- `GET /ready` — PostgreSQL ping + auth mode
- `GET /api/v1/runtime/metrics` — pool, leases, recovery queue (authenticated)

## Rollback

1. Keep the previous `axiom-server` binary.
2. Do not reverse additive migrations unless you have a tested down-script (none ships).
3. Restore PostgreSQL from backup if a data fix is required (`docs/deployment/backup-restore.md`).

## Resource limits

`AXIOM_MAX_BODY_BYTES` (default 4 MiB), `AXIOM_DB_MAX_CONNECTIONS`, `AXIOM_OTLP_RATE_PER_MINUTE`, `AXIOM_WORKER_REQUEST_TIMEOUT_SECONDS`, `AXIOM_RETENTION_DAYS` (project default 30; enforcement of deletion is operator-run, not automatic purge in v0.9).
