# Authorization

Every sensitive HTTP handler calls `Scope::require(Permission)` after authentication. Studio UI hiding is not the control plane.

## Roles

| Role | Capabilities |
|---|---|
| `viewer` | Read projects, runs, workflows, approvals, shared context, metrics |
| `operator` | Viewer + run/workflow execute, telemetry, A2A, shared writes. **Cannot approve.** |
| `approver` | Viewer + approval decisions |
| `admin` | Operator + approver + API keys, members, workflow edits, audit log |
| `owner` | All permissions |

Database `project_members` overrides JWT role hints. API keys carry their own `role`.

## Tenant isolation

Run-scoped queries filter by `project_id`. Guessed UUIDs return `404` for the caller's project, not another tenant's row. Claiming `X-Axiom-Project-Id` without membership is `403`. Worker poll checks project ownership before using an in-memory handle.

Create project copies the caller's organization and inserts the creator as `owner`.

## Endpoints

Approvals, runs, workflows, evidence, shared context, hierarchy, telemetry, API keys, A2A, and SSE all require a matching permission. `/health` and `/ready` stay unauthenticated so load balancers can probe them.

Security events are written for key lifecycle, membership changes, and approval decisions (`GET /api/v1/security-events` needs `AuditRead`).
