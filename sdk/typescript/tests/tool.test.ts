import { describe, expect, test } from "bun:test";
import { z } from "zod";
import { Agent, SchemaValidationError, ToolError, defineTool } from "../src";

describe("tools", () => {
  test("validates typed inputs", async () => {
    const t = defineTool({
      name: "add",
      schema: z.object({ a: z.number(), b: z.number() }),
      jsonSchema: { type: "object", properties: { a: { type: "number" }, b: { type: "number" } }, required: ["a", "b"] },
      execute: async ({ a, b }) => ({ sum: a + b }),
    });
    const out = (await t.invoke({ a: 1, b: 2 }, { idempotencyKey: null, timeoutMs: 1000 })) as { sum: number };
    expect(out.sum).toBe(3);
    expect(() => t.validate({ a: "x" } as never)).toThrow(SchemaValidationError);
  });

  test("times out", async () => {
    const t = defineTool({
      name: "slow",
      schema: z.object({}),
      jsonSchema: { type: "object" },
      timeoutMs: 30,
      execute: async () => {
        await new Promise((r) => setTimeout(r, 200));
        return { ok: true };
      },
    });
    await expect(t.invoke({}, { idempotencyKey: null, timeoutMs: 30 })).rejects.toBeInstanceOf(ToolError);
  });

  test("serializes action contracts for the runtime", () => {
    const t = defineTool({
      name: "submit_refund",
      schema: z.object({ order_id: z.string() }),
      jsonSchema: { type: "object", properties: { order_id: { type: "string" } }, required: ["order_id"] },
      effect: "IDEMPOTENT_WRITE",
      idempotent: true,
      risk: "high",
      permissions: ["payments:refund"],
      requiresApproval: true,
      execute: async () => ({ ok: true }),
    });
    const spec = t.toSpec();
    expect(spec.effect).toBe("IDEMPOTENT_WRITE");
    expect(spec.requires_approval).toBe(true);
    expect(spec.required_permissions).toEqual(["payments:refund"]);
  });
});

describe("Agent", () => {
  test("rejects unqualified model ids", () => {
    expect(() => new Agent({ name: "x", model: "gpt-4o" })).toThrow(/provider-qualified/);
  });

  test("serializes tools", () => {
    const lookup = defineTool({
      name: "lookup",
      schema: z.object({ id: z.string() }),
      jsonSchema: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
      execute: async ({ id }) => ({ id }),
    });
    const agent = new Agent({ name: "a", model: "mock/deterministic", tools: [lookup] });
    expect(agent.toDefinition().tools[0]?.name).toBe("lookup");
    expect(agent.modelId).toBe("mock/deterministic");
  });
});
