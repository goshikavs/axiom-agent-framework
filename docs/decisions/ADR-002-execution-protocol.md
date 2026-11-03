# ADR-002: Framework-neutral execution event protocol

- Status: Accepted
- Date: 2026-10-27

## Context

Studio must correlate intent, execution, actual state, and verification evidence for
native Goshikq runs and for runs produced by external frameworks (LangGraph, OpenAI Agents
SDK, OpenTelemetry sources, …). External events may be late, duplicated, or out of order.

## Decision

A single versioned event envelope is the unit of persistence and streaming. It is defined
in Rust (`crates/goshikq-protocol`), exported as JSON Schema to
`protocol/schemas/execution-event.v1.json`, and mirrored by generated/checked types in the
Python and TypeScript SDKs.

```json
{
  "schema_version": "1.0",
  "event_id": "uuid",
  "run_id": "uuid",
  "trace_id": "string",
  "span_id": "string",
  "parent_span_id": "string | null",
  "sequence": 42,
  "event_type": "tool.completed",
  "timestamp": "RFC 3339 UTC (source time)",
  "agent_id": "string | null",
  "step_id": "string | null",
  "source_framework": "goshikq | langgraph | openai-agents | otel | ...",
  "status": "succeeded | failed | ... | null",
  "duration_ms": 125,
  "payload": {},
  "metadata": {}
}
```

Rules:

1. `event_id` is the deduplication key. Ingestion is idempotent: re-sending an event with
   the same `event_id` is a no-op (`ON CONFLICT DO NOTHING`), and the batch response
   reports `accepted` vs `duplicates`.
2. `sequence` is allocated per run by the runtime for native runs (monotonic, gap-free).
   For external runs, the collector allocates ingestion sequence numbers in arrival order
   and preserves the source `timestamp`; Studio orders by `(timestamp, sequence)` and
   labels the ordering as *source time*.
3. `timestamp` is the source timestamp; `ingested_at` is stored separately in the
   `execution_events` table and returned by the API.
4. `parent_span_id` expresses causal parentage. The collector never fabricates parents:
   events whose parent is unknown are shown attached to the run root and flagged
   `orphaned_parent` in Studio.
5. Event type names are a closed list per schema version (see `EventType` in
   `goshikq-protocol`). Adding a type is a minor schema bump (1.x) and old readers must
   ignore unknown types; removing or renaming is a major bump.
6. Payload shapes for native events are documented per type in `docs/architecture/execution-protocol.md`.
   Payloads from external frameworks are stored verbatim under `payload` with
   `metadata.raw_source` describing the mapping used.
7. Events are written in the same database transaction as the state change they
   describe. An `event_outbox` table records pending publication; the in-process
   publisher broadcasts to SSE subscribers and marks the row published. Delivery to
   subscribers is at-least-once; subscribers deduplicate on `event_id` and resume with
   `Last-Event-ID` (= sequence).

## Consequences

- Studio can render any run, native or external, from one table and one stream.
- Native runs have strong ordering; external runs have best-effort ordering, labeled as such.
- Schema evolution is explicit and tested (`tests/contract`).
