/**
 * Interop: official `@modelcontextprotocol/sdk` 1.32.2 vs examples/mcp-inventory-server.
 *
 * The maintained SDK stdio transport is newline-delimited JSON (not LSP
 * Content-Length). This test exercises initialize + tools/list + tools/call.
 * Streamable HTTP, OAuth, and later MCP features are not claimed.
 */
import { describe, expect, test } from "bun:test";
import { join } from "node:path";

const server = join(import.meta.dir, "../../../examples/mcp-inventory-server/server.ts");

describe("official MCP SDK client ↔ inventory server", () => {
  test("initialize, list, and call over NDJSON stdio", async () => {
    const { Client } = await import("@modelcontextprotocol/sdk/client/index.js");
    const { StdioClientTransport } = await import("@modelcontextprotocol/sdk/client/stdio.js");
    const transport = new StdioClientTransport({
      command: process.execPath,
      args: [server],
    });
    const client = new Client({ name: "axiom-official-sdk-test", version: "0.8" });
    const connected = await Promise.race([
      client.connect(transport).then(() => true as const),
      new Promise<"timeout">((resolve) => setTimeout(() => resolve("timeout"), 8000)),
    ]);
    expect(connected).toBe(true);
    if (connected === "timeout") {
      await client.close().catch(() => undefined);
      return;
    }
    try {
      const caps = client.getServerCapabilities();
      expect(caps).toBeDefined();
      const tools = await client.listTools();
      expect(tools.tools.map((t: { name: string }) => t.name)).toContain("inventory_lookup");
      const ok = await client.callTool({ name: "inventory_lookup", arguments: { sku: "SKU-1" } });
      expect(ok).toBeDefined();
      const missing = await client.callTool({ name: "does_not_exist", arguments: {} }).catch((e: unknown) => e);
      expect(missing).toBeDefined();
    } finally {
      await client.close();
    }
  });
});
