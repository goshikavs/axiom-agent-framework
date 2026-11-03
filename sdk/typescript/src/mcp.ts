import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { PermissionDeniedError, SchemaValidationError, ToolError } from "./errors.ts";
import { Evidence, withEvidence } from "./evidence.ts";
import { jsonable } from "./json.ts";
import { defineTool, type Effect, type Risk, type Tool, Verifier } from "./tool.ts";
import { z, type ZodType } from "zod";

export type McpTransport =
  | { kind: "stdio"; command: string; args?: string[]; cwd?: string; env?: Record<string, string> }
  | { kind: "http"; url: string; headers?: Record<string, string>; authorization?: string };

export type McpToolPolicy = {
  /** Exact MCP tool names permitted. Descriptions cannot add names. */
  allowlist: string[];
  permissions?: string[];
  effect?: Effect | string;
  risk?: Risk | string;
  requiresApproval?: boolean;
  timeoutMs?: number;
  maxPayloadBytes?: number;
  idempotent?: boolean;
  trust?: "authoritative" | "derived" | "untrusted" | "inaccessible";
};

export type McpServerOptions = {
  id: string;
  transport: McpTransport;
  /** Remote HTTP endpoints must be allowlisted; stdio is local-only. */
  allowedEndpoints?: string[];
  policy: McpToolPolicy;
  /** Per-tool policy. MCP descriptions cannot supply these values. */
  toolPolicies?: Record<string, Partial<McpToolPolicy>>;
};

export type DiscoveredMcpTool = {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
};

type JsonRpc = { jsonrpc: "2.0"; id?: number; method?: string; params?: unknown; result?: unknown; error?: { code: number; message: string } };

const SECRET_KEYS = /password|passwd|secret|api[_-]?key|token|authorization|credential/i;

export class McpClient {
  readonly id: string;
  readonly policy: McpToolPolicy;
  readonly toolPolicies: Record<string, Partial<McpToolPolicy>>;
  capabilities: Record<string, unknown> = {};
  tools: DiscoveredMcpTool[] = [];
  private transport: McpJsonRpc;
  private nextId = 1;
  private connected = false;

  constructor(opts: McpServerOptions) {
    this.id = opts.id;
    this.policy = opts.policy;
    this.toolPolicies = opts.toolPolicies ?? {};
    if (opts.transport.kind === "http") {
      assertAllowedEndpoint(opts.transport.url, opts.allowedEndpoints ?? []);
      if (!opts.transport.authorization && !opts.transport.headers?.authorization) {
        throw new ToolError("remote MCP connections require Authorization", { code: "unauthenticated_mcp" });
      }
    }
    this.transport = createTransport(opts.transport);
  }

  async connect(): Promise<void> {
    await this.transport.start();
    const init = (await this.call("initialize", {
      protocolVersion: "2024-11-05",
      capabilities: { roots: { listChanged: false } },
      clientInfo: { name: "goshikq-typescript", version: "0.1.0-alpha" },
    })) as Record<string, unknown>;
    this.capabilities = (init.capabilities as Record<string, unknown>) ?? {};
    await this.notify("notifications/initialized", {});
    const listed = (await this.call("tools/list", {})) as { tools?: Array<Record<string, unknown>> };
    this.tools = (listed.tools ?? []).map((t) => ({
      name: String(t.name ?? ""),
      description: String(t.description ?? ""),
      inputSchema: (t.inputSchema as Record<string, unknown>) ?? { type: "object" },
    }));
    this.connected = true;
  }

  async close(): Promise<void> {
    this.connected = false;
    await this.transport.close();
  }

  discover(): DiscoveredMcpTool[] {
    return this.tools.filter((t) => this.policy.allowlist.includes(t.name));
  }

  toGoshikqTools(): Tool<any, any>[] {
    return this.discover().map((t) => this.wrap(t));
  }

  private wrap(discovered: DiscoveredMcpTool): Tool<any, any> {
    const schema = z.record(z.string(), z.unknown()) as ZodType<Record<string, unknown>>;
    const policy: McpToolPolicy = { ...this.policy, ...this.toolPolicies[discovered.name], allowlist: this.policy.allowlist };
    const client = this;
    const serverId = this.id;
    return defineTool({
      name: discovered.name,
      description: discovered.description,
      schema,
      jsonSchema: discovered.inputSchema,
      effect: policy.effect ?? (discovered.name.includes("refund") ? "IDEMPOTENT_WRITE" : "READ_ONLY"),
      risk: policy.risk ?? "medium",
      timeoutMs: policy.timeoutMs ?? 15_000,
      maxAttempts: policy.idempotent ? 2 : 1,
      idempotent: policy.idempotent ?? false,
      requiresApproval: policy.requiresApproval ?? false,
      permissions: policy.permissions ?? [],
      execute: async (input, ctx) => {
        const started = Date.now();
        if (!policy.allowlist.includes(discovered.name)) {
          throw new PermissionDeniedError(`MCP tool \`${discovered.name}\` is not allowlisted`);
        }
        if (policy.permissions?.length) {
          // Permission enforcement is owned by the Rust runtime via ToolSpec.required_permissions.
        }
        const payload = JSON.stringify(input);
        const max = policy.maxPayloadBytes ?? 64 * 1024;
        if (Buffer.byteLength(payload, "utf8") > max) {
          throw new SchemaValidationError(`MCP payload exceeds ${max} bytes`);
        }
        validateAgainstJsonSchema(discovered.inputSchema, input);
        const raw = (await client.call(
          "tools/call",
          { name: discovered.name, arguments: input },
          ctx.timeoutMs,
        )) as Record<string, unknown>;
        if (raw.isError) {
          throw new ToolError(`MCP tool \`${discovered.name}\` returned isError`, {
            code: "mcp_tool_error",
            retryable: false,
            details: redact(raw),
          });
        }
        const observedAt = new Date().toISOString();
        const content = raw.content ?? raw;
        const structured = (raw.structuredContent as Record<string, unknown> | undefined) ?? asRecord(content);
        const evidenced = withEvidence(structured, [
          new Evidence({
            source_system: `mcp:${serverId}`,
            resource_id: `${discovered.name}`,
            resource_version: typeof raw._meta === "object" && raw._meta ? String((raw._meta as { version?: string }).version ?? "") || null : null,
            trust: policy.trust ?? "untrusted",
            observed_at: observedAt,
            keys: [discovered.name],
          }),
        ]);
        evidenced._mcp = {
          server_id: serverId,
          tool: discovered.name,
          schema_valid: true,
          permissions: policy.permissions ?? [],
          risk: policy.risk ?? "medium",
          approval_required: policy.requiresApproval ?? false,
          duration_ms: Date.now() - started,
          effect: policy.effect ?? "READ_ONLY",
        };
        return evidenced;
      },
      verifier: async (_args, output) => {
        if (output && typeof output === "object" && "_evidence" in (output as object)) {
          return Verifier.inconclusive("MCP outputs are observations, not authoritative business state");
        }
        return Verifier.inconclusive("MCP tool has no business verifier");
      },
    });
  }

  private async call(method: string, params: unknown, timeoutMs = 20_000): Promise<unknown> {
    const id = this.nextId++;
    const msg: JsonRpc = { jsonrpc: "2.0", id, method, params };
    const resp = await this.transport.request(msg, timeoutMs);
    if (resp.error) throw new ToolError(`MCP ${method}: ${resp.error.message}`, { code: "mcp_rpc_error", details: resp.error });
    return resp.result;
  }

  private async notify(method: string, params: unknown): Promise<void> {
    await this.transport.notify({ jsonrpc: "2.0", method, params });
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  if (value && typeof value === "object" && !Array.isArray(value)) return value as Record<string, unknown>;
  return { content: jsonable(value) as unknown };
}

function redact(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(redact);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = SECRET_KEYS.test(k) ? "[REDACTED]" : redact(v);
    }
    return out;
  }
  return value;
}

function assertAllowedEndpoint(url: string, allowed: string[]): void {
  if (!allowed.length) throw new ToolError("remote MCP url is not on the endpoint allowlist", { code: "endpoint_denied" });
  const ok = allowed.some((prefix) => url === prefix || url.startsWith(prefix));
  if (!ok) throw new ToolError(`MCP endpoint ${url} is not allowed`, { code: "endpoint_denied" });
  const u = new URL(url);
  if (u.protocol !== "https:" && u.hostname !== "127.0.0.1" && u.hostname !== "localhost") {
    throw new ToolError("remote MCP requires https or loopback", { code: "endpoint_denied" });
  }
}

function validateAgainstJsonSchema(schema: Record<string, unknown>, value: unknown): void {
  if (schema.type === "object") {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new SchemaValidationError("MCP arguments must be an object");
    }
    const required = (schema.required as string[] | undefined) ?? [];
    for (const key of required) {
      if (!(key in (value as Record<string, unknown>))) {
        throw new SchemaValidationError(`missing required MCP argument \`${key}\``);
      }
    }
    if (schema.additionalProperties === false) {
      const props = Object.keys((schema.properties as Record<string, unknown>) ?? {});
      for (const key of Object.keys(value as Record<string, unknown>)) {
        if (!props.includes(key)) throw new SchemaValidationError(`unexpected MCP argument \`${key}\``);
      }
    }
  }
}

interface McpJsonRpc {
  start(): Promise<void>;
  close(): Promise<void>;
  request(msg: JsonRpc, timeoutMs: number): Promise<JsonRpc>;
  notify(msg: JsonRpc): Promise<void>;
}

function createTransport(t: McpTransport): McpJsonRpc {
  if (t.kind === "stdio") return new StdioTransport(t);
  return new HttpTransport(t);
}

class HttpTransport implements McpJsonRpc {
  constructor(private readonly t: Extract<McpTransport, { kind: "http" }>) {}
  async start(): Promise<void> {
    /* no-op */
  }
  async close(): Promise<void> {
    /* no-op */
  }
  async request(msg: JsonRpc, timeoutMs: number): Promise<JsonRpc> {
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), timeoutMs);
    try {
      const headers: Record<string, string> = {
        "content-type": "application/json",
        accept: "application/json",
        ...(this.t.headers ?? {}),
      };
      if (this.t.authorization) headers.authorization = this.t.authorization.startsWith("Bearer ") ? this.t.authorization : `Bearer ${this.t.authorization}`;
      const resp = await fetch(this.t.url, { method: "POST", headers, body: JSON.stringify(msg), signal: ctrl.signal });
      if (!resp.ok) throw new ToolError(`MCP HTTP ${resp.status}`, { code: "mcp_http_error" });
      return (await resp.json()) as JsonRpc;
    } finally {
      clearTimeout(timer);
    }
  }
  async notify(msg: JsonRpc): Promise<void> {
    await this.request({ ...msg, id: undefined }, 5_000).catch(() => undefined);
  }
}

class StdioTransport implements McpJsonRpc {
  private child?: ChildProcessWithoutNullStreams;
  private buf = Buffer.alloc(0);
  private pending = new Map<number, { resolve: (v: JsonRpc) => void; reject: (e: Error) => void }>();
  constructor(private readonly t: Extract<McpTransport, { kind: "stdio" }>) {}

  async start(): Promise<void> {
    this.child = spawn(this.t.command, this.t.args ?? [], {
      cwd: this.t.cwd,
      env: { ...process.env, ...this.t.env },
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    this.child.stdout.on("data", (chunk: Buffer) => this.onData(chunk));
    this.child.stderr.on("data", () => undefined);
    this.child.on("exit", (code) => {
      for (const p of this.pending.values()) p.reject(new Error(`MCP server exited (${code})`));
      this.pending.clear();
    });
  }

  async close(): Promise<void> {
    this.child?.kill();
    this.child = undefined;
  }

  async request(msg: JsonRpc, timeoutMs: number): Promise<JsonRpc> {
    if (!this.child) throw new ToolError("MCP stdio transport not started", { code: "mcp_disconnected" });
    const id = msg.id;
    if (typeof id !== "number") throw new ToolError("MCP request missing id", { code: "mcp_rpc_error" });
    const p = new Promise<JsonRpc>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new ToolError(`MCP request timed out after ${timeoutMs}ms`, { code: "timeout", retryable: false }));
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (v) => {
          clearTimeout(timer);
          resolve(v);
        },
        reject: (e) => {
          clearTimeout(timer);
          reject(e);
        },
      });
    });
    this.write(msg);
    return p;
  }

  async notify(msg: JsonRpc): Promise<void> {
    this.write(msg);
  }

  private write(msg: JsonRpc): void {
    // Official MCP SDK 1.32+ stdio is newline-delimited JSON, not LSP Content-Length.
    this.child!.stdin.write(Buffer.from(`${JSON.stringify(msg)}\n`, "utf8"));
  }

  private onData(chunk: Buffer): void {
    this.buf = Buffer.concat([this.buf, chunk]);
    while (this.consumeFrame()) {
      /* drain */
    }
  }

  private consumeFrame(): boolean {
    if (!this.buf.length) return false;
    const textStart = this.buf.toString("utf8", 0, Math.min(this.buf.length, 32)).replace(/^\s+/, "");
    if (textStart.startsWith("Content-Length:") || textStart.toLowerCase().startsWith("content-length:")) {
      const headerEnd = this.buf.indexOf("\r\n\r\n");
      if (headerEnd < 0) return false;
      const header = this.buf.subarray(0, headerEnd).toString("utf8");
      const match = /Content-Length:\s*(\d+)/i.exec(header);
      if (!match) {
        this.buf = this.buf.subarray(headerEnd + 4);
        return true;
      }
      const len = Number(match[1]);
      const start = headerEnd + 4;
      if (this.buf.length < start + len) return false;
      this.dispatchJson(this.buf.subarray(start, start + len).toString("utf8"));
      this.buf = this.buf.subarray(start + len);
      return true;
    }
    const nl = this.buf.indexOf("\n");
    if (nl < 0) return false;
    const line = this.buf.subarray(0, nl).toString("utf8").replace(/\r$/, "").trim();
    this.buf = this.buf.subarray(nl + 1);
    if (line) this.dispatchJson(line);
    return true;
  }

  private dispatchJson(raw: string): void {
    try {
      const msg = JSON.parse(raw) as JsonRpc;
      if (typeof msg.id === "number") {
        const pending = this.pending.get(msg.id);
        if (pending) {
          this.pending.delete(msg.id);
          pending.resolve(msg);
        }
      }
    } catch {
      /* ignore malformed frames */
    }
  }
}
