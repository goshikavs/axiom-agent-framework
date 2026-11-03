# Multi-agent development

Requires `goshikq-server` on `GOSHIKQ_RUNTIME_URL` (default `http://127.0.0.1:7400`) and `PYTHONPATH=sdk/python/src` (or `pip install -e sdk/python`).

## Python

```python
from goshikq_ai import Agent, AgentGroup, AgentTask, CoordinationPolicy, Goal
from goshikq_ai.models import MockModel, ModelTurn

group = AgentGroup(
    supervisor,
    policy=CoordinationPolicy(mode="sequential", max_depth=4, max_delegations=8),
    state_provider=provider,
)
result = await group.run(parent_goal, [
    AgentTask(eligibility, child_goal, role="eligibility"),
    AgentTask(inspection, child_goal, role="inspection"),
])
assert result.parent.verified  # parent StateProvider, not child reports
```

TypeScript: `AgentGroup` / `AgentTask` in `@goshikq/sdk` (`sdk/typescript/src/group.ts`). Same worker protocol.

## Examples

```text
python examples/multi-agent-refund/main.py
python examples/multi-agent-insurance/main.py --mutate-policy
python examples/a2a-collaboration/main.py
```

## APIs

- `POST /api/v1/runs` with `defer_start: true` — coordinator
- `POST /api/v1/runs/{id}/tasks` — child
- `POST /api/v1/runs/{id}/start` — spawn deferred executor
- `GET /api/v1/runs/{id}/hierarchy` — parent + children from `run_tasks`
- `PUT/GET /api/v1/runs/{id}/shared-context`
- `POST /api/v1/runs/{id}/handoff`
- `GET /.well-known/agent-card.json`, `POST /a2a`

Studio: open the parent execution. The **Multi-agent** panel lists persisted children and delegation events. Graph/timeline selection stays the existing synchronized inspector.
