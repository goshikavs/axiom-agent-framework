# TypeScript SDK

The TypeScript SDK (`@goshikq/sdk`) defines agents, tools, goals, and a mock model. It does **not** run its own execution loop. The Rust runtime remains canonical. The SDK process answers worker requests:

| Kind | SDK responsibility |
| --- | --- |
| `model.generate` | `ModelAdapter.generate` |
| `tool.execute` | validated tool function + forwarded `idempotency_key` |
| `state.snapshot` | `StateProvider.snapshot` |
| `goal.evaluate` | success predicate + invariants |
| `action.verify` | tool verifier against a runtime-provided snapshot |

## Install

From the repository (Bun workspace):

```bash
bun install
cd sdk/typescript
bun test
bun run typecheck
```

Node.js 20+ can import the same TypeScript sources. Library code uses `fetch` and `AbortSignal`, not Bun-only APIs.

```ts
import { Agent, Goal, MockModel, Runtime, StateSnapshot, defineTool } from "@goshikq/sdk";
```

## First agent

See `templates/typescript-agent` and `examples/typescript-refund-agent`.

```bash
# from the repo root, with goshikq-server already on :7400
goshikq init --template typescript
bun install
bun run start
```

Or run the refund example:

```bash
goshikq run examples/typescript-refund-agent
```

A successful refund prints `COMPLETED/VERIFIED` and a `run_id`. Open `/executions/<run_id>` in Studio. Origin shows **Native TypeScript SDK**.

## Tool registration

```ts
const submit = defineTool({
  name: "submit_refund",
  schema: z.object({ order_id: z.string(), amount_cents: z.number() }),
  jsonSchema: { type: "object", properties: { order_id: { type: "string" }, amount_cents: { type: "number" } }, required: ["order_id", "amount_cents"], additionalProperties: false },
  effect: "IDEMPOTENT_WRITE",
  idempotent: true,
  risk: "high",
  permissions: ["payments:refund"],
  execute: async (input, ctx) => processor.submit(input, ctx.idempotencyKey),
  verifier: (args, output, state) => { /* authoritative state only */ },
});
```

`effect`, `idempotent`, and `requiresApproval` are Goshikq contracts. They are not inferred from the function body or from MCP descriptions.

## Cancellation and timeouts

`Runtime.run(goal, { signal, timeoutMs })` aborts polling and cancels the run. Tool `timeoutMs` is enforced in the worker; the runtime also has its own deadline.

## Crash recovery

The runtime resumes **persisted** dispatch state (idempotency keys, action journal, checkpoints). Arbitrary TypeScript closures are not replayed. Persist business records yourself (the refund example keeps them in `SimulatedStore`).

## Public abstractions

`Agent`, `Goal`, `Tool` / `defineTool`, `StateProvider`, `Runtime`, `ModelAdapter` / `MockModel`, `ExecutionResult`, `VerificationResult`, `ActionContract`, `EvidenceReference`.
