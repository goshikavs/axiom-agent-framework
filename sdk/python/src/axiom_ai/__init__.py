"""Axiom Python SDK.

Build agents, declare goals and action contracts, and execute them on the Axiom runtime
with full observability and outcome verification.

Quick start::

    from axiom_ai import Agent, Goal, Runtime, tool
    from axiom_ai.models import MockModel, ModelTurn

    @tool(effect="read_only")
    async def lookup_order(order_id: str) -> dict:
        return {"order_id": order_id, "status": "paid"}

    agent = Agent(name="hello", model=MockModel(script=[ModelTurn(content="done")]))
    result = Runtime(agent).run_sync(Goal(description="Say hello"))
    print(result.status)
"""

from ._version import __version__
from .a2a import A2A_PROTOCOL_VERSION, A2aClient
from .agent import Agent, ExecutionLimits, RetryPolicy
from .errors import AxiomError, RetryableToolError, RuntimeConnectionError, ToolError
from .goal import Goal, InvariantOutcome, Verification
from .group import AgentGroup, AgentTask, CoordinationPolicy, GroupResult
from .evidence import DecisionRef, Evidence, resource_hint, with_evidence
from .result import RunResult
from .runtime import Runtime
from .state import StateProvider, StateSnapshot
from .tools import Effect, Risk, Tool, tool

__all__ = [
    "A2A_PROTOCOL_VERSION",
    "A2aClient",
    "Agent",
    "AgentGroup",
    "AgentTask",
    "AxiomError",
    "CoordinationPolicy",
    "GroupResult",
    "DecisionRef",
    "Effect",
    "Evidence",
    "ExecutionLimits",
    "Goal",
    "InvariantOutcome",
    "RetryPolicy",
    "RetryableToolError",
    "Risk",
    "RunResult",
    "Runtime",
    "RuntimeConnectionError",
    "StateProvider",
    "StateSnapshot",
    "Tool",
    "ToolError",
    "Verification",
    "__version__",
    "resource_hint",
    "tool",
    "with_evidence",
]
