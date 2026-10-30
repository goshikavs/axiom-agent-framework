# OpenTelemetry

See also [architecture/otlp-ingestion.md](../architecture/otlp-ingestion.md).

## Endpoint

```text
POST http://127.0.0.1:7400/v1/traces
POST http://127.0.0.1:7400/api/v1/otlp/v1/traces
Content-Type: application/json
# or application/x-protobuf  (ExportTraceServiceRequest)
```

Live suite: `python -m pytest tests/otel/test_otlp_live.py -q` (7 passed against PostgreSQL).

Imported runs are **UNVERIFIED**. Mapping `otel-genai-v1` does not emit goal verification events.

## Outbound export (Axiom's own spans)

Set `AXIOM_OTEL_EXPORTER_OTLP_ENDPOINT` (or `OTEL_EXPORTER_OTLP_ENDPOINT`) to an OTLP/HTTP collector. Axiom POSTs JSON `ExportTraceServiceRequest` to `{endpoint}/v1/traces`.

Exported names include `run.*`, `tool.*`, `verification.*`, `recovery.*`, `goal.satisfied` / `goal.failed`. Attributes: run id, event type, agent id, status. **Payloads, model I/O, and secrets are omitted.** Sampling: `AXIOM_OTEL_TRACES_SAMPLER_ARG` (0.0–1.0, per `run_id`).

Canonical execution events remain in PostgreSQL. Export is a derived view.

Test: `python -m pytest tests/otel/test_runtime_export.py` against a loopback collector.

## Limits and isolation

- Max payload size and 2000 spans per request (excess → 400).
- `X-Axiom-Project-Id` must match the authenticated project or the request is 403.
- Duplicate spans reuse deterministic `event_id`.
- Out-of-order spans keep producer timestamps; sequence is arrival order.
