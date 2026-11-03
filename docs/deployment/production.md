# Production configuration

## Required

```text
DATABASE_URL=postgres://...
GOSHIKQ_BIND=0.0.0.0:7400
GOSHIKQ_AUTH_MODE=oidc
GOSHIKQ_OIDC_ISSUER=https://idp.example.com/...
GOSHIKQ_OIDC_AUDIENCE=goshikq
GOSHIKQ_OIDC_JWKS_URL=https://idp.example.com/.../certs
GOSHIKQ_LOG_FORMAT=json
GOSHIKQ_CORS_ORIGINS=https://studio.example.com
GOSHIKQ_CAPTURE_MODEL_IO=redacted
```

Never set `GOSHIKQ_AUTH_MODE=none` on a non-loopback address without `GOSHIKQ_ALLOW_INSECURE_BIND` (unsafe).

## Migrations

```bash
goshikq-server --migrate-only
# or start the server; it migrates on boot
```

Additive SQL lives in `crates/goshikq-storage/migrations/`. v0.9 adds `0007_tenancy.sql`.

## Readiness

- `GET /health` — process up
- `GET /ready` — PostgreSQL ping + auth mode
- `GET /api/v1/runtime/metrics` — pool, leases, recovery queue (authenticated)

## Rollback

1. Keep the previous `goshikq-server` binary.
2. Do not reverse additive migrations unless you have a tested down-script (none ships).
3. Restore PostgreSQL from backup if a data fix is required (`docs/deployment/backup-restore.md`).

## Resource limits

`GOSHIKQ_MAX_BODY_BYTES` (default 4 MiB), `GOSHIKQ_DB_MAX_CONNECTIONS`, `GOSHIKQ_OTLP_RATE_PER_MINUTE`, `GOSHIKQ_WORKER_REQUEST_TIMEOUT_SECONDS`, `GOSHIKQ_RETENTION_DAYS` (project default 30; enforcement of deletion is operator-run, not automatic purge in v0.9).
