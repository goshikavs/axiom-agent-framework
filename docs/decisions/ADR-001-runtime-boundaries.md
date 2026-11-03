# ADR-001: Runtime boundaries between Rust, SDKs, and Studio

- Status: Accepted
- Date: 2026-10-27

## Context

Goshikq must be a genuine framework with its own execution lifecycle, model-independent,
and usable from Python and TypeScript. Developers write agents, tools, model adapters,
state providers, and goal predicates in their own language. The runtime must enforce
limits, permissions, approvals, and state transitions that model output cannot override.

Two placements were considered for the execution loop:

1. **SDK-owned loop**: each SDK re-implements planning/validation/verification and reports
   events to a collector. Simple to start, but every SDK duplicates safety-critical logic,
   and the Rust engine degrades into a passive trace store.
2. **Runtime-owned loop with SDK workers**: the Rust runtime owns the state machine and
   asks the SDK process to perform the pieces only it can perform (call the model, run a
   tool, read authoritative state, evaluate a predicate).

## Decision

Option 2. The Rust runtime (`goshikq-runtime`, served by `goshikq-server`) owns:

- Run state transitions and their validation (`goshikq-core::state_machine`)
- Step scheduling, max-step / deadline / token-budget enforcement
- Action lifecycle (`REQUESTED → VALIDATED → … → SUCCEEDED | FAILED | TIMED_OUT | RECONCILIATION_REQUIRED | CANCELLED`)
- Tool-argument JSON Schema validation and permission checks
- Approval gating and approval decisions
- Retry rules that depend on effect category and idempotency
- Checkpoints, event persistence, verification orchestration
- The HTTP API, SSE streams, and OpenAPI document

SDKs (Python `goshikq_ai`, TypeScript `@goshikq/sdk`) own:

- `Agent`, `Goal`, `tool` definitions and their JSON Schemas
- Model adapters (OpenAI, Anthropic, deterministic mock, future providers)
- Tool implementations
- `StateProvider` implementations and goal predicates / invariants
- Submitting the run definition and serving **worker requests**

### Worker protocol

After `POST /api/v1/runs` the SDK attaches as a worker for that run:

```
POST /api/v1/runs/{run_id}/worker/poll?timeout_ms=20000
  -> { "request": { "request_id", "kind", "payload" } | null, "run_status", "terminal" }
POST /api/v1/runs/{run_id}/worker/respond
  <- { "request_id", "ok", "result" | "error" }
```

Request kinds in protocol 1.0: `model.generate`, `tool.execute`, `state.snapshot`,
`goal.evaluate`, `action.verify`. The runtime never executes model-generated code; it
only dispatches typed requests to tools that were registered when the run was created.

Long-polling over HTTP was chosen over WebSockets for the first version because it is
trivially resumable, proxy-friendly, debuggable with `curl`, and needs no additional
client dependency. A WebSocket transport can be added without changing request kinds.

Studio owns visualization, inspection, and controlled runtime actions (cancel, approval
decisions). The browser never talks to the runtime with provider credentials; Studio's
Next.js server proxies the API.

## Consequences

- One implementation of the safety-critical loop, tested in Rust independently of UI.
- SDKs stay thin; adding a language means implementing the worker protocol and adapters.
- A run cannot progress without a connected worker. If no worker responds within the
  configured worker timeout the run moves to `RECOVERY_REQUIRED` (see ADR-003).
- Latency per step includes one HTTP round-trip per worker request. Benchmarks must
  separate this overhead from model latency.
