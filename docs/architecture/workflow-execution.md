# Workflow execution

Saved graphs are executed by the **existing Rust run executor**. Next.js never schedules work.

## Path

1. `POST /api/v1/workflows` stores a draft (invalid graphs allowed when `draft: true`).
2. `POST /api/v1/workflows/{id}/publish` validates and writes an immutable `workflow_versions` row.
3. `POST /api/v1/workflows/{id}/run` creates a normal run whose `metadata.workflow.definition` is the **pinned snapshot**.
4. `RunExecutor::execute` detects the pinned definition and calls `execute_workflow` in `crates/axiom-runtime/src/workflow_exec.rs`.
5. Node state is persisted in `workflow_node_executions` and listed at `GET /api/v1/runs/{id}/workflow-nodes`.
6. The run appears in the standard execution explorer.

## Node mapping

| Node | Runtime behavior |
| --- | --- |
| START | Copies run input into context |
| AGENT | Native marker by default (`completed: true`). Set `config.spawn: true` to create a child run via `create_child_task`. Spawned children need a worker; without one the parent fails `WORKER_UNAVAILABLE` (not success). |
| TOOL | Native `echo` / declared `result` for demos. `handler: registered` (or any non-echo/fail handler with `config.tool`) dispatches through the action journal and worker protocol. Missing workers fail `WORKER_UNAVAILABLE` without dispatch. |
| CONDITION | Small expression language (`true`/`false`, JSON pointers, `==` / `!=`) |
| PARALLEL | Fork marker; successors run when ready |
| JOIN | `all` or `any` of incoming predecessors |
| APPROVAL | Existing approval row + waiter (`WAITING_FOR_APPROVAL`) |
| WAIT | Durable `WAITING_FOR_EXTERNAL_STATE` until `duration_ms` |
| VERIFY | JSON predicate on workflow context; does not treat child SUCCESS as VERIFIED |
| END | Completes the coordinator run |

## Recovery and idempotency

Completed nodes are not re-executed. A node that crashed while `running` a `NON_IDEMPOTENT_WRITE` / `IRREVERSIBLE` tool becomes `RECOVERY_REQUIRED`. Agent children reuse `idempotency_key=wf:{run}:{node}`. Editing a workflow after publish does not mutate in-flight runs; they keep the pinned snapshot.

## Events

Canonical `ExecutionEvent` types: `run.started`, `agent.delegated`, `approval.*`, `verification.*`, `tool.started` / `tool.completed` with `payload.workflow_node`.
