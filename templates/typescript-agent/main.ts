import { Agent, Goal, MockModel, Runtime, StateSnapshot, defineTool } from "@axiom-ai/sdk";
import { z } from "zod";

const echo = defineTool({
  name: "echo",
  description: "Return the input text. Read-only.",
  schema: z.object({ text: z.string() }),
  jsonSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"], additionalProperties: false },
  effect: "READ_ONLY",
  execute: async ({ text }) => ({ echoed: text }),
});

const agent = new Agent({
  name: "hello_ts",
  model: new MockModel({
    script: [
      { tool_calls: [["echo", { text: "hello from TypeScript" }]] },
      { content: "Echoed the greeting." },
    ],
  }),
  tools: [echo],
  instructions: "Call echo once, then stop.",
});

const goal = new Goal({
  description: "Echo a greeting",
  success: (state: StateSnapshot) => Boolean(state.get("ok")),
  maxSteps: 4,
});

const runtime = new Runtime(agent, {
  stateProvider: {
    snapshot: () => new StateSnapshot({ state: { ok: true }, version: "1", source: "template" }),
  },
  studioUrl: process.env.AXIOM_STUDIO_URL || "http://127.0.0.1:3000",
});

const result = await runtime.run(goal);
console.log(result.summary());
console.log(`run_id=${result.runId}`);
if (result.studioUrl) console.log(`studio=${result.studioUrl}`);
