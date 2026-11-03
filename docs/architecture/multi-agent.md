# Native multi-agent model (v0.7)

Goshikq treats collaborating agents as **parent and child runs**, not as a second executor. The Rust runtime remains the authority for transitions, journals, evidence, and goal verification.

## Abstractions

| Type | Role |
|---|---|
| `AgentGroup` | SDK coordinator. Creates a deferred parent run, then child tasks. |
| `AgentTask` | One child agent + goal + input. |
| `Delegation` | `POST /api/v1/runs/{id}/tasks` — persisted `run_tasks` row. |
| `Handoff` | `POST /api/v1/runs/{id}/handoff` — successor child + `agent.handoff_completed`. |
| `ParentGoal` / `ChildGoal` | Independent `GoalDefinition`s. Child success is not parent verification. |
| `SharedContext` | Versioned key/value with OCC, scoped to a run id. |
| `CoordinationPolicy` | `sequential` / `parallel` / `supervisor`, `max_depth`, `max_delegations`, `prevent_cycles`. |

Each child has a unique `task_id` and a `correlation_id` (defaults to the parent run id). Children reuse the parent `trace_id`.

## Scheduling

- Sequential: SDK runs children one after another, then `POST /start` on the parent.
- Parallel: SDK `asyncio.gather` / `Promise.allSettled`, then parent start.
- Supervisor/worker: parent stays `CREATED` (`defer_start: true`) until children finish.

`defer_start` runs are **not** marked `RECOVERY_REQUIRED` on process restart. Parent/child rows persist; `GET /api/v1/runs/{id}/hierarchy` reconstructs the tree.

## Bounds

- `max_depth` is taken from the **root** coordinator's `metadata.coordination` (walked through ancestors).
- Creating a child whose agent name already appears in the ancestor chain is rejected when `prevent_cycles` is true.
- `max_delegations` limits children of one parent.
- Duplicate `idempotency_key` returns the existing child (`reused: true`).

There is no unbounded agent-to-agent loop.

## Cancellation, deadlines, budgets

Canceling a parent cancels non-terminal descendants (recursive SQL, then per-child cancel or direct `CANCELLED` transition). Child `execution_limits.deadline_seconds` and token/cost budgets are the existing per-run limits. Human approval: `requires_approval: true` holds the child until `/api/v1/approvals/{id}/decision`; the approval arguments include `child_run_id` (untrusted remote metadata cannot set verification).

## What this is not

Natural-language “shared memory” is not business state. Use `shared_context` and the existing evidence graph. A child run reaching `COMPLETED` / `VERIFIED` does not write the parent’s `verification_status`.
