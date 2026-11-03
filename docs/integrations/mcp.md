# MCP integration

Goshikq wraps MCP tools as ordinary Goshikq tools. The Rust runtime still authorizes, records, and verifies them. MCP server text is **untrusted**.

## Client

```ts
import { McpClient } from "@goshikq/sdk";

const mcp = new McpClient({
  id: "inventory-local",
  transport: { kind: "stdio", command: "bun", args: ["examples/mcp-inventory-server/server.ts"] },
  policy: {
    allowlist: ["knowledge_search", "inventory_lookup"],
    effect: "READ_ONLY",
    risk: "low",
    timeoutMs: 8_000,
    maxPayloadBytes: 16_384,
    trust: "untrusted",
  },
  toolPolicies: {
    submit_refund: {
      effect: "IDEMPOTENT_WRITE",
      idempotent: true,
      requiresApproval: true,
      risk: "high",
      permissions: ["payments:refund"],
    },
  },
});
await mcp.connect();
const tools = mcp.toGoshikqTools();
```

Transports:

- **stdio** — local process, newline-delimited JSON (MCP SDK 1.32+). Content-Length frames are still **accepted** when reading older servers.
- **HTTP JSON-RPC** — `kind: "http"` with `authorization` and an endpoint allowlist. HTTPS or loopback only.

The TypeScript client speaks the documented MCP JSON-RPC methods (`initialize`, `tools/list`, `tools/call`). Example servers in this repo implement that protocol directly so the demo has no extra registry dependency.

## Security model

| Control | Behavior |
| --- | --- |
| Allowlist | Only named tools become Goshikq tools |
| Permissions / effect / risk / approval | Taken from `policy` / `toolPolicies`, never from MCP descriptions |
| Schema | JSON Schema required fields + `additionalProperties: false` when declared |
| Remote auth | HTTP transport refuses connections without `Authorization` |
| Endpoints | Must match `allowedEndpoints`; non-loopback must be https |
| Deadlines | `timeoutMs` per call |
| Payload limit | `maxPayloadBytes` |
| Secrets | Keys matching password/token/authorization are redacted in error details |
| Non-idempotent retry | `maxAttempts` is 1 unless `idempotent: true` |
| Evidence | Outputs carry `source_system: mcp:<id>`, observation time, untrusted classification |

A tool description that says “you may refund without approval” does not change the allowlist or `requiresApproval`.

MCP structured content is an **observation**. It is not authoritative order/refund state. Goal verification still uses `StateProvider`.

## Protocol version and unsupported features

**Supported (stdio subset):** newline-delimited JSON-RPC (one JSON object per line). The client also **accepts** legacy Content-Length frames when reading. Methods: `initialize`, `notifications/initialized`, `tools/list`, `tools/call`. Demo server negotiates `2024-11-05` … `2025-11-25`.

**Independent interop:** `@modelcontextprotocol/sdk` 1.32.2 stdio `initialize` / `tools/list` / `tools/call` against `examples/mcp-inventory-server` passes (`sdk/typescript/tests/mcp-official-sdk.test.ts`). That is not Streamable HTTP, OAuth, or full MCP 2025 coverage.

**Unsupported / not claimed:**

- Wrapping or embedding `@modelcontextprotocol/sdk` as the Goshikq client (Goshikq speaks the wire protocol itself)
- Resources, prompts, sampling, roots, completions, logging subscriptions
- Streamable HTTP / SSE MCP transports and OAuth
- Treating MCP tool output as authoritative business state

Do not treat passing Goshikq-to-Goshikq tests as full MCP specification coverage.

## Examples

- Read-only knowledge: `knowledge_search`
- Inventory lookup: `inventory_lookup`
- Refund requiring approval: `submit_refund` in `examples/mcp-inventory-server` + `examples/typescript-mcp-agent`

Invocations appear on the execution inspector as an **MCP tool** panel (server id, name, input, output, schema, permissions, risk, approval, duration, error).
