# Framework adapters (v0.4)

## LangGraph

`examples/langgraph-refund` compiles a real `StateGraph`. Node functions record OTLP spans (OpenInference / GenAI attributes) and POST them to Axiom. LangGraph remains the executor. Axiom does not intercept node side effects.

Requires `pip install langgraph` and a running `axiom-server`.

## OpenAI Agents SDK

`examples/openai-agents-refund` uses the documented `TracingProcessor` hooks when `openai-agents` is installed. Without the package it emits the same OTLP shape from a mock path so the mapping can be tested.

Private SDK internals are not used.

## Capability matrix

See [framework-capability-manifest.md](framework-capability-manifest.md) and `GET /api/v1/capabilities`.
