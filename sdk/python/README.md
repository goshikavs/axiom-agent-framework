# goshikq-ai (Python SDK)

Python SDK for [Goshikq](https://github.com/goshikavs/axiom-agent-framework), the outcome-driven,
model-independent AI agent framework. The SDK declares agents, goals, and tool contracts; the
Goshikq Rust runtime owns execution, policy enforcement, persistence, and verification.

Status: `0.1.0a1` (alpha). APIs may change before `0.1.0`.

## Install

```bash
pip install -e ".[dev]"            # from sdk/python in the repo
pip install -e ".[openai]"         # optional provider adapters
pip install -e ".[anthropic]"
```

Requires a running Goshikq runtime (default `http://127.0.0.1:7400`, override with `GOSHIKQ_RUNTIME_URL`).

## Minimal example

```python
from goshikq_ai import Agent, Goal, Runtime, StateSnapshot, tool
from goshikq_ai.models import MockModel, ModelTurn

@tool(effect="read_only")
def lookup_order(order_id: str) -> dict:
    return {"order_id": order_id, "status": "paid"}

def order_was_looked_up(state: StateSnapshot) -> bool:
    return state.get("lookups") == 1

agent = Agent(
    name="hello-agent",
    model=MockModel(script=[ModelTurn(tool_calls=[("lookup_order", {"order_id": "A-1"})]), ModelTurn(content="done")]),
    tools=[lookup_order],
)
result = Runtime(agent, state_provider=...).run_sync(Goal(description="Look up order A-1", success=order_was_looked_up))
print(result.status, result.verification_status)
```

`result.verified` is `True` only when the runtime completed the run **and** the goal predicate held
against a fresh `StateSnapshot`. Without a state provider the run ends `UNVERIFIED`.

## Concepts

| Type | Purpose |
| --- | --- |
| `@tool(effect=..., risk=..., requires_approval=..., verifier=...)` | Action contract. `effect` drives retry safety in the runtime. |
| `Goal(description, success=, invariants=)` | Predicates evaluated by the runtime against authoritative state, never model claims. |
| `StateProvider` | Returns a `StateSnapshot` of the real system (DB, API) on request. |
| `Agent(name, model, tools)` | `model` is `"provider/model"` or a `ModelAdapter` (e.g. `MockModel`). |
| `Runtime(agent, state_provider=)` | Submits the run and serves worker requests until the run is terminal. |

## Development

```bash
pytest
ruff check src tests
mypy
```
