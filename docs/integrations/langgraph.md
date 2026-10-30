# LangGraph

`examples/langgraph-refund/main.py` builds a real LangGraph `StateGraph` and exports OTLP spans to Axiom. LangGraph executes the nodes. Axiom does not intercept writes or treat the import as a verified refund.

```text
pip install langgraph
$env:AXIOM_RUNTIME_URL = "http://127.0.0.1:7400"
$env:PYTHONPATH = "<repo>\sdk\python\src"
python examples/langgraph-refund/main.py
```

Live import on this host: run `ce34aa20-2d1a-5036-b8ff-9ddbb016c67c`, status **UNVERIFIED**, verification **NOT_EVALUATED**, `source_framework=langgraph`. Studio: `/executions?framework=langgraph`.

The first classified CHAIN span may become `agent_name` (for example `lookup`). That is telemetry classification, not Axiom controlling the graph.
