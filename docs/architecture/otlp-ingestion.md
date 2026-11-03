# OTLP ingestion

Goshikq accepts the public OTLP/HTTP trace protocol:

- `POST /v1/traces` (standard collector path)
- `POST /api/v1/otlp/v1/traces` (same handler behind the API prefix)

Content types:

- `application/x-protobuf` — `ExportTraceServiceRequest`
- `application/json` — OTLP JSON mapping of the same message

gRPC is not implemented.

Operator guide: [docs/integrations/opentelemetry.md](../integrations/opentelemetry.md). Live v0.4 commands: [docs/milestones/v0.4-report.md](../milestones/v0.4-report.md).

## Mapping `otel-genai-v1`

Implemented in `goshikq_core::otel` (IO-free). Span classification:

| Signal | Event types |
|---|---|
| Root / `invoke_agent` | `run.started` + `run.completed` or `run.failed` |
| GenAI chat / OpenInference LLM | `model.completed` / `model.failed` |
| `execute_tool` / tool name | `tool.completed` / `tool.failed` |
| Other | `run.status_changed` with `payload.unmapped_span=true` |
| `goshikq.state_snapshot` attribute | `state.snapshot` (optional, never inferred) |

Rules:

- Unknown attributes are stored (after redaction).
- Missing token counts, models, and timestamps stay absent. They are listed under `metadata.missing_attributes`.
- Prompts/completions are redacted unless `GOSHIKQ_CAPTURE_MODEL_IO=full`.
- Duplicate spans reuse deterministic `event_id` (UUID v5 of trace+span+type).
- Out-of-order spans keep source timestamps; the collector assigns arrival `sequence`.
- Parent span ids are preserved. Missing parents are not synthesized.
- Goal verification events are never emitted from traces.
- External `run.completed` maps to run status **UNVERIFIED**, not COMPLETED/VERIFIED.

## Security

Production must set `GOSHIKQ_AUTH_MODE=api_key`. The receiver accepts `Authorization: Bearer` or `x-api-key`. `X-Goshikq-Project-Id` and resource attribute `goshikq.project_id` must match the authenticated project.

Limits: `GOSHIKQ_MAX_BODY_BYTES` (default 4 MiB), `GOSHIKQ_OTLP_MAX_SPANS` (2000), `GOSHIKQ_OTLP_RATE_PER_MINUTE` (600). Retention follows the project `retention_days` column; no separate OTLP store.

Unauthenticated public ingestion is refused when the server is bound off loopback (existing bind guard).
