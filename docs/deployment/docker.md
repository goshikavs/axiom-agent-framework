# Docker

Compose file: `docker-compose.yml`. Images:

- Runtime: `Dockerfile` (`goshikq-server`)
- Studio: `apps/studio/Dockerfile`

```bash
docker compose up -d postgres
# Smoke (GitHub Actions ubuntu-latest; requires Docker)
GOSHIKQ_AUTH_MODE=none GOSHIKQ_ALLOW_INSECURE_BIND=true \
  bash scripts/docker-smoke.sh
```

The runtime image does **not** copy `.env`. Pass OIDC at runtime. Production must not use `GOSHIKQ_AUTH_MODE=none`.

On the v0.9.1 authoring host Docker Desktop was **not installed**, so image build/smoke is **NOT RUN**. The `docker-smoke` GitHub Actions job is defined and has **not** executed (no remote). Do not claim a published image or a passing compose smoke test.

Local development without Docker: `DATABASE_URL=embedded` or Postgres on `127.0.0.1:55432`.
