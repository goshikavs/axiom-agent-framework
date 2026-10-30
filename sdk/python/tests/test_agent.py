from axiom_ai import Agent, ExecutionLimits
from axiom_ai.errors import ConfigurationError
from axiom_ai.models import MockModel, ModelTurn
from axiom_ai.tools import tool


@tool
def ping() -> str:
    return "pong"


def test_definition_roundtrip() -> None:
    agent = Agent(
        name="n",
        model=MockModel(script=[ModelTurn(content="hi")]),
        instructions="be brief",
        tools=[ping],
        execution_limits=ExecutionLimits(max_steps=3),
    )
    d = agent.to_definition()
    assert d["name"] == "n"
    assert d["model"].startswith("mock/")
    assert d["tools"][0]["name"] == "ping"
    assert d["tools"][0]["effect"] == "READ_ONLY"
    assert d["execution_limits"]["max_steps"] == 3


def test_unqualified_model_rejected() -> None:
    try:
        Agent(name="n", model="gpt-4o")
        raise AssertionError("expected ConfigurationError")
    except ConfigurationError as e:
        assert "provider-qualified" in str(e)


def test_duplicate_tools_rejected() -> None:
    try:
        Agent(name="n", model=MockModel(script=[ModelTurn(content="x")]), tools=[ping, ping])
        raise AssertionError("expected ConfigurationError")
    except ConfigurationError:
        pass
