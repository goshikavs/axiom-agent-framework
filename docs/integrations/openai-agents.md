# OpenAI Agents SDK (tracing-only)

**Compatibility label: tracing-only.** Tested with `openai-agents` 0.24.0 `TracingProcessor` ingest. Axiom does **not** execute `Runner.run` as the runtime, intercept SDK tool calls, or treat ingested traces as `VERIFIED`. A real offline `Runner.run` has not been demonstrated on this host.

Axiom does not execute or approve OpenAI Agents SDK tools. The integration imports traces the SDK emits through public tracing processor hooks (`agents.tracing.TracingProcessor`, `add_trace_processor`).

Imported runs stay `UNVERIFIED` / `NOT_EVALUATED` unless a separate Axiom native run verifies authoritative state.

## Example

`examples/openai-agents-refund/main.py` uses the real SDK when `agents` is installed, otherwise a local mock that still emits OTLP. The mock path is a fallback, not a compatibility claim.

```bash
python -m pip install openai-agents
python examples/openai-agents-refund/main.py
```

## Test

`tests/otel/test_openai_agents_sdk.py` requires a reachable `axiom-server` and the `agents` package.

- Always: register a public `TracingProcessor`, ingest OTLP, assert the run is not `VERIFIED`.
- Offline `Runner.run`: attempted via `agents.models.fake_model.FakeModel` when that type exists in the installed version.
- Paid provider calls are not required and are not made in the default test.

If `Runner.run` cannot complete offline, the test still ingests processor-emitted (or explicitly labeled processor-only) spans and records the limitation instead of substituting hand-written OTLP as “full SDK compatibility.”
