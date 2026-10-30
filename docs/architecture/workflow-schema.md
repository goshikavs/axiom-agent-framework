# Workflow schema

Canonical types live in `crates/axiom-core/src/workflow.rs`. Studio, the CLI, and the runtime deserialize the same JSON.

## Identity

| Field | Meaning |
| --- | --- |
| `name` | Human name |
| `description` | Free text |
| `version` | Working-copy integer; published snapshots are stored separately |
| `id` | Catalog UUID (assigned on save; ignored on import until stored) |

## Nodes

`start`, `agent`, `tool`, `condition`, `parallel`, `join`, `approval` (alias `human_approval`), `wait`, `verify` (alias `verification`), `end`.

Each node has `id`, `kind`, `label`, `config`, optional `max_iterations`, optional `input_schema` / `output_schema`, optional `timeout_ms`.

## Edges

`id`, `source`, `target`, optional `label` (required on condition branches), optional `mapping` (`from`/`to` JSON pointers).

## Policies

- `error_policy.on_error`: `fail_fast` \| `continue`
- `timeouts.run_seconds`, `node_ms`, `approval_seconds`, `wait_seconds`
- `retry_policy.max_attempts`, `backoff_ms`
- `required_permissions`
- `goal.verification_policy`: `none` \| `json_predicate` \| `authoritative`
- `goal.required` plus a `verify` node when verification is required

Imported JSON is untrusted: it is schema-validated before publish or run. Drafts may be invalid.

See `docs/architecture/workflow-execution.md` for how a **published** version is pinned onto a run.
