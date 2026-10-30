from axiom_ai import AgentGroup, AgentTask, CoordinationPolicy, Goal
from axiom_ai.agent import Agent
from axiom_ai.models import MockModel, ModelTurn


def test_policy_dict() -> None:
    p = CoordinationPolicy(mode="parallel", max_depth=3)
    d = p.to_dict()
    assert d["mode"] == "parallel"
    assert d["max_depth"] == 3
    assert d["prevent_cycles"] is True
    assert d["require_parent_verification"] is True


def test_task_wraps_agent() -> None:
    a = Agent(name="w", model=MockModel(script=[ModelTurn(content="ok")]))
    t = AgentTask(a, Goal(description="child"), role="worker")
    assert t.role == "worker"
    assert t.agent.name == "w"


def test_group_constructs() -> None:
    sup = Agent(name="s", model=MockModel(script=[ModelTurn(content="ok")]))
    g = AgentGroup(sup, policy=CoordinationPolicy(mode="sequential"))
    assert g.policy.mode == "sequential"
    assert g.supervisor.name == "s"
