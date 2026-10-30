# @axiom-ai/sdk

TypeScript SDK for Axiom. Agents, tools, goals, and the mock model live here. **Execution, retries, evidence, and verification run in the Rust runtime** over the worker protocol (`POST /api/v1/runs/{id}/worker/poll` + `respond`). This package is not a second agent engine.

```bash
bun install
bun test
bun run typecheck
```

Node.js 20+ can import the same sources (`fetch`, `AbortSignal`). Do not use Bun-only APIs in library code.

```ts
import { Agent, Goal, MockModel, Runtime, defineTool } from "@axiom-ai/sdk";
import { z } from "zod";

const echo = defineTool({
  name: "echo",
  schema: z.object({ text: z.string() }),
  jsonSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
  execute: async ({ text }) => ({ echoed: text }),
});

const agent = new Agent({
  name: "hello",
  model: new MockModel({ script: [{ tool_calls: [["echo", { text: "hi" }]] }, { content: "done" }] }),
  tools: [echo],
});

const result = await new Runtime(agent, {
  stateProvider: { snapshot: () => new StateSnapshot({ state: { ok: true }, source: "demo" }) },
}).run(new Goal({ description: "echo", success: () => true }));
```

See `examples/typescript-refund-agent` for a verified refund and `docs/guides/typescript-sdk.md`.
