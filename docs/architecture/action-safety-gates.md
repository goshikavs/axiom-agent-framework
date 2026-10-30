# Action safety gates

Deterministic gates run in the Rust executor. The model cannot override them.

## Revalidation policies

| Policy | Behavior |
|---|---|
| `SELECTIVE` | Declared-graph `compute_impact` (v0.3 default) |
| `FULL_DECLARED` | Any registered hit → recheck every declared decision/action/approval |
| `SELECTIVE_PLUS_PREWRITE` | Selective walk ∪ conventional action `resource_id` version check |
| `STRICT_CONTRACT` | Declared coverage + selective walk + prewrite + risk rules |

Unit tests require these four to differ on a fixture. On the v0.3.1 seeded generator, `SELECTIVE_PLUS_PREWRITE` often matches `SELECTIVE` because action `resource_id`s are already impact seeds. That is a property of the generator, not an alias in code.

## High-risk writes (`high` / `critical`, or non-idempotent / irreversible)

- Missing required evidence → block (ignores `on_missing=allow`)
- Stale required evidence → block
- Conflicted evidence → block
- Inconclusive required verification → block
- Invalidated approval cannot authorize the write
- Graph revalidation still runs after a complete contract so coverage cannot skip a stale dependent write

## Read-only / low-risk

`on_missing_evidence` may be `warn` or `allow`. Warnings are recorded; the write/read proceeds unless the contract says `block`.

## Independent fields

| Field | Role |
|---|---|
| Dispatch / tool status | What the invocation did |
| Coverage status | Declared requirements |
| Gate decision | Allow / warn / block / require approval |
| Goal verification | Authoritative success predicate |

A coverage-complete gate `ALLOW` is not a verified goal.
