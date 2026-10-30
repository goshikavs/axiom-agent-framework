import asyncio

from axiom_ai.models import MockModel, ModelTurn, last_tool_result


def test_script_tool_then_text() -> None:
    model = MockModel(
        script=[
            ModelTurn(tool_calls=[("lookup", {"id": "1"})]),
            ModelTurn(content="done"),
        ]
    )

    async def run() -> None:
        first = await model.generate([], [{"name": "lookup", "parameters": {}}], {})
        assert first.tool_calls[0].name == "lookup"
        assert first.tool_calls[0].arguments == {"id": "1"}
        assert first.usage is not None
        second = await model.generate([], [], {})
        assert second.content == "done"
        assert second.tool_calls == []

    asyncio.run(run())


def test_policy_reads_tool_result() -> None:
    def policy(messages, tools, idx):
        if idx == 0:
            return ModelTurn(tool_calls=[("echo", {"n": 1})])
        result = last_tool_result(messages, "echo")
        return ModelTurn(content=f"saw {result}")

    model = MockModel(policy=policy)

    async def run() -> None:
        await model.generate([], [], {})
        msgs = [{"role": "tool", "name": "echo", "content": '{"ok": true}'}]
        second = await model.generate(msgs, [], {})
        assert "ok" in (second.content or "")

    asyncio.run(run())
