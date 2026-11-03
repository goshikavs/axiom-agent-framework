# Upgrade and migration

Goshikq is `0.1.0-alpha`. Expect breaking API changes until a v1.0 tag exists.

## Database

Migrations are additive and run on `goshikq-server` startup (or `--migrate-only`).

| Version | File | Notes |
| --- | --- | --- |
| v0.1–v0.8 | `0001`–`0006` | Core, recovery, evidence, multi-agent, workflows |
| v0.9 | `0007_tenancy.sql` | Members, API keys, security events |
| v0.9.1 | `0008_retention.sql` | Index for admin retention purge |

Restore from `pg_dump` before applying a new binary if you need a rollback path (`docs/deployment/backup-restore.md`).

## Configuration

New in v0.9.1 (all optional except when `GOSHIKQ_AUTH_MODE=oidc`):

- `GOSHIKQ_OIDC_JWKS_URL` (required for production OIDC; HMAC test mode is loopback-only)
- `GOSHIKQ_OIDC_JWKS_CACHE_SECONDS`
- `GOSHIKQ_OTEL_EXPORTER_OTLP_ENDPOINT`
- `GOSHIKQ_OTEL_TRACES_SAMPLER_ARG`
- `POST /api/v1/admin/retention/purge` (owner)

## SDKs

Install from a built wheel/tarball, not an editable checkout, when validating a release:

```bash
python -m build --wheel sdk/python
pip install dist/goshikq_ai-*.whl
bun pm pack   # in sdk/typescript
bun add ./goshikq-ai-sdk-0.1.0-alpha.tgz
```

Nothing is published to PyPI or npm in this milestone.
