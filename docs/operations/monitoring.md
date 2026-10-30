# Monitoring

## Endpoints

| Path | Auth | Use |
|---|---|---|
| `/health` | no | Liveness |
| `/ready` | no | PostgreSQL + process |
| `/api/v1/runtime/metrics` | yes | Pool, leases, recovery queue, failed actions, workflow counters |
| `/api/v1/metrics/overview` | yes | Product metrics for Studio |

## Logs

`AXIOM_LOG_FORMAT=json` emits tracing-subscriber JSON. Do not log `Authorization` or raw evidence payloads. Default capture mode redacts secret-like fields.

## OpenTelemetry

Agent/framework **ingest** of OTLP/HTTP is implemented (v0.4). Runtime **self-export** of spans to an OTLP collector is not compiled in v0.9; use JSON logs plus `/api/v1/runtime/metrics`. Claiming a full OTEL SDK pipeline would be inaccurate.

## Alerts worth wiring

- `/ready` ≠ 200
- `recovery_queue` > 0 for longer than the lease TTL
- `leases.expired` growing
- `failed_actions` spikes
- JWT/JWKS fetch failures in logs (`oidc jwt rejected`)
