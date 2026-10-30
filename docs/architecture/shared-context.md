# Shared context

Versioned, access-controlled key/value store for multi-agent business facts. It is **not** a chat transcript and not a second evidence system.

## Storage

Migration `0005_multi_agent.sql`:

- `shared_context` — `(project_id, scope_id, key)` unique, `version` bigint, `owner_agent`, `visibility` (`private`/`team`/`public`), `evidence_refs` JSON array of resource ids already in the evidence graph.
- `shared_context_history` — immutable row per write.

`scope_id` is a run id in the caller’s project (typically the parent). Cross-project reads fail like every other run-scoped query.

## Optimistic concurrency

`PUT /api/v1/runs/{id}/shared-context`

```json
{ "key": "claim", "value": {"status": "open"}, "expected_version": 1, "owner_agent": "coord", "visibility": "team", "evidence_refs": ["claim:CLM-77"] }
```

- First write: omit `expected_version` (insert version 1).
- Update: `expected_version` must match. Mismatch → `409 conflict`.
- History is append-only.

## Evidence

`evidence_refs` point at existing evidence resource ids. Change-impact still runs on the evidence graph when tools record `Evidence` / `DecisionRef`. Shared context does not infer dependencies from prose.
