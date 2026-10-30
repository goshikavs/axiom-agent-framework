# Cross-language worker protocol

Canonical types live in `crates/axiom-protocol/src/worker.rs` (protocol 1.0). Python (`sdk/python/src/axiom_ai/worker.py`) and TypeScript (`sdk/typescript/src/worker.ts`) implement the same HTTP loop.

```
SDK  POST /api/v1/runs                → run_id
SDK  POST /api/v1/runs/{id}/worker/poll?timeout_ms=…
RT   { request?, run_status, terminal }
SDK  POST /api/v1/runs/{id}/worker/respond
     { request_id, ok, result | error }
```

## Registration and identity

On `create_run`, TypeScript sends `source_framework: axiom-typescript` and metadata:

```json
{
  "sdk_version": "0.1.0-alpha",
  "protocol_version": "1.0",
  "worker": {
    "identity": "<uuid>",
    "language": "typescript",
    "capabilities": ["model.generate", "tool.execute", "state.snapshot", "goal.evaluate", "action.verify"]
  }
}
```

Poll/respond also send `User-Agent: axiom-typescript/…`, `x-axiom-worker-id`, and `x-axiom-worker-capabilities`. The runtime does not require those headers to function; they identify the worker for logs and Studio.

## Request kinds

Internally tagged JSON: `{"kind":"tool.execute","payload":{...}}` flattened onto `WorkerRequest` with `request_id`, `run_id`, `issued_at`, `timeout_ms`.

`tool.execute.idempotency_key` is forwarded unchanged on retries of the same logical action.

## Errors

```json
{ "code": "timeout", "message": "…", "retryable": false, "details": null }
```

`retryable: true` is only legal when the worker knows the side effect did not occur.

## Disconnect

If the SDK stops polling, the runtime lease and request timeout paths apply. The worker does not silently retry non-idempotent tools. Reconnection is a new poll loop on the same `run_id` while the run is non-terminal.

## Recovery

Supported: resume from checkpoints and the action journal, reuse idempotency keys, do not duplicate external effects the processor honors. Unsupported: reconstructing an in-flight TypeScript function that never recorded state.

## Shared status strings

Execution, goal, verification, tool, dispatch, recovery, evidence coverage, and error codes are the protocol enums. See `sdk/typescript/src/result.ts` `TERMINAL_STATUSES` and `axiom_ai.result.TERMINAL_STATUSES`. Conformance: `tests/conformance/test_protocol_status.py`.
