import { describe, expect, test } from "bun:test";
import { z } from "zod";
import { Agent, Goal, MockModel, Runtime, StateSnapshot, defineTool } from "../src";

async function runtimeUp(): Promise<boolean> {
  try {
    const base = (process.env.AXIOM_RUNTIME_URL || "http://127.0.0.1:7400").replace(/\/$/, "");
    const r = await fetch(`${base}/health`, { signal: AbortSignal.timeout(1500) });
    return r.ok;
  } catch {
    return false;
  }
}

describe("TypeScript worker against Rust runtime", () => {
  test("executes a mock agent to VERIFIED", async () => {
    if (!(await runtimeUp())) return;
    const echo = defineTool({
      name: "echo",
      schema: z.object({ text: z.string() }),
      jsonSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"], additionalProperties: false },
      effect: "READ_ONLY",
      execute: async ({ text }) => ({ echoed: text }),
    });
    const agent = new Agent({
      name: "ts_live_echo",
      model: new MockModel({
        script: [{ tool_calls: [["echo", { text: "ping" }]] }, { content: "done" }],
      }),
      tools: [echo],
    });
    const goal = new Goal({
      description: "echo ping",
      success: (s: StateSnapshot) => s.get("ok") === true,
      maxSteps: 4,
    });
    const rt = new Runtime(agent, {
      stateProvider: { snapshot: () => new StateSnapshot({ state: { ok: true }, version: "1", source: "live-test" }) },
    });
    const result = await rt.run(goal, { timeoutMs: 60_000 });
    expect(result.status).toBe("COMPLETED");
    expect(result.verificationStatus).toBe("VERIFIED");
    expect(result.runId).toMatch(/[0-9a-f-]{36}/);
  }, 90_000);

  test("idempotency key is stable across retries of submit", async () => {
    if (!(await runtimeUp())) return;
    const keys: string[] = [];
    const t = defineTool({
      name: "mark",
      schema: z.object({ n: z.number() }),
      jsonSchema: { type: "object", properties: { n: { type: "number" } }, required: ["n"] },
      effect: "IDEMPOTENT_WRITE",
      idempotent: true,
      execute: async (_input, ctx) => {
        keys.push(ctx.idempotencyKey ?? "");
        return { ok: true };
      },
    });
    const agent = new Agent({
      name: "ts_live_idem",
      model: new MockModel({ script: [{ tool_calls: [["mark", { n: 1 }]] }, { content: "done" }] }),
      tools: [t],
    });
    const goal = new Goal({
      description: "mark once",
      success: () => true,
      maxSteps: 4,
    });
    const rt = new Runtime(agent, { stateProvider: { snapshot: () => new StateSnapshot({ state: {}, version: "1", source: "t" }) } });
    const a = await rt.run(goal, { idempotencyKey: `ts-idem-${Date.now()}` });
    expect(a.terminal).toBe(true);
    expect(keys[0]?.length).toBeGreaterThan(8);
  }, 90_000);

  test("cancellation stops the worker", async () => {
    if (!(await runtimeUp())) return;
    const t = defineTool({
      name: "hang",
      schema: z.object({}),
      jsonSchema: { type: "object" },
      timeoutMs: 30_000,
      execute: async (_i, ctx) => {
        await new Promise((resolve, reject) => {
          const timer = setTimeout(resolve, 30_000);
          ctx.signal?.addEventListener("abort", () => {
            clearTimeout(timer);
            reject(new Error("cancelled"));
          });
        });
        return {};
      },
    });
    const agent = new Agent({
      name: "ts_live_cancel",
      model: new MockModel({ script: [{ tool_calls: [["hang", {}]] }, { content: "nope" }] }),
      tools: [t],
    });
    const ctrl = new AbortController();
    const rt = new Runtime(agent, { stateProvider: { snapshot: () => new StateSnapshot({ state: { ok: true }, version: "1", source: "t" }) } });
    setTimeout(() => ctrl.abort(), 500);
    await expect(
      rt.run(new Goal({ description: "hang", success: () => true, maxSteps: 3 }), { signal: ctrl.signal, timeoutMs: 8_000 }),
    ).rejects.toThrow();
  }, 30_000);
});
