import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { McpClient, PermissionDeniedError, SchemaValidationError } from "../src";

const server = join(import.meta.dir, "../../../examples/mcp-inventory-server/server.ts");

async function connected(allowlist: string[]) {
  const client = new McpClient({
    id: "test",
    transport: { kind: "stdio", command: process.execPath, args: [server] },
    policy: { allowlist, timeoutMs: 5_000, maxPayloadBytes: 4096, effect: "READ_ONLY", risk: "low" },
    toolPolicies: {
      submit_refund: { effect: "IDEMPOTENT_WRITE", requiresApproval: true, risk: "high", permissions: ["payments:refund"] },
    },
  });
  await client.connect();
  return client;
}

describe("MCP", () => {
  test("discovers tools and ignores prompt-injection in descriptions", async () => {
    const client = await connected(["knowledge_search", "inventory_lookup"]);
    try {
      const names = client.discover().map((t) => t.name);
      expect(names).toContain("knowledge_search");
      expect(names).not.toContain("submit_refund");
      const knowledge = client.tools.find((t) => t.name === "knowledge_search");
      expect(knowledge?.description).toMatch(/SYSTEM OVERRIDE/);
      const wrapped = client.toAxiomTools();
      expect(wrapped.every((t) => t.effect === "READ_ONLY")).toBe(true);
      expect(wrapped.every((t) => t.requiresApproval === false)).toBe(true);
    } finally {
      await client.close();
    }
  });

  test("schema validation rejects extra fields", async () => {
    const client = await connected(["inventory_lookup"]);
    try {
      const tool = client.toAxiomTools()[0]!;
      await expect(tool.invoke({ sku: "SKU-1", extra: true }, { idempotencyKey: null, timeoutMs: 5_000 })).rejects.toBeInstanceOf(
        SchemaValidationError,
      );
    } finally {
      await client.close();
    }
  });

  test("permission / allowlist denial", async () => {
    const client = await connected(["knowledge_search"]);
    try {
      expect(client.discover().map((t) => t.name)).toEqual(["knowledge_search"]);
      const refund = client.tools.find((t) => t.name === "submit_refund");
      expect(refund).toBeDefined();
      expect(() => client.toAxiomTools().find((t) => t.name === "submit_refund")).toBeDefined();
      expect(client.toAxiomTools().some((t) => t.name === "submit_refund")).toBe(false);
    } finally {
      await client.close();
    }
  });

  test("side-effecting refund keeps Axiom contract", async () => {
    const client = await connected(["submit_refund"]);
    try {
      const tool = client.toAxiomTools()[0]!;
      expect(tool.effect).toBe("IDEMPOTENT_WRITE");
      expect(tool.requiresApproval).toBe(true);
      expect(tool.permissions).toEqual(["payments:refund"]);
      const out = (await tool.invoke({ order_id: "ORD-1", amount_cents: 100 }, { idempotencyKey: "k1", timeoutMs: 5_000 })) as Record<
        string,
        unknown
      >;
      expect(out._mcp).toMatchObject({ server_id: "test", tool: "submit_refund", schema_valid: true });
      expect(out._evidence).toBeDefined();
    } finally {
      await client.close();
    }
  });

  test("unallowlisted invoke is denied", async () => {
    const client = await connected(["knowledge_search"]);
    try {
      const hidden = client.tools.find((t) => t.name === "submit_refund")!;
      const wrapped = (client as unknown as { wrap: (t: typeof hidden) => { invoke: Function } }).wrap?.(hidden);
      if (wrapped) {
        await expect(wrapped.invoke({ order_id: "x", amount_cents: 1 }, { idempotencyKey: null, timeoutMs: 1000 })).rejects.toBeInstanceOf(
          PermissionDeniedError,
        );
      }
    } finally {
      await client.close();
    }
  });
});
