import { describe, expect, test } from "bun:test";
import { z } from "zod";
import { Agent, defineTool } from "../src";

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
  });
});
