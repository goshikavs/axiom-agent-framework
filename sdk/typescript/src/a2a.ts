import { RuntimeConnectionError, RuntimeRejectedError } from "./errors.ts";

export const A2A_PROTOCOL_VERSION = "1.0";

export class A2aClient {
  private rpcId = 0;
  constructor(
    readonly endpoint: string,
    readonly apiKey?: string,
  ) {}

  async agentCard(): Promise<Record<string, unknown>> {
    const base = this.endpoint.replace(/\/a2a\/?$/, "");
    const resp = await fetch(`${base}/.well-known/agent-card.json`);
    if (!resp.ok) throw new RuntimeRejectedError("a2a_card", await resp.text(), resp.status);
    return (await resp.json()) as Record<string, unknown>;
  }

  sendMessage(text: string, opts: { contextId?: string; idempotencyKey?: string } = {}): Promise<Record<string, unknown>> {
    const params: Record<string, unknown> = {
      message: { role: "user", parts: [{ kind: "text", text }] },
    };
    if (opts.contextId) {
      params.contextId = opts.contextId;
      (params.message as Record<string, unknown>).contextId = opts.contextId;
    }
    if (opts.idempotencyKey) params.idempotencyKey = opts.idempotencyKey;
    return this.rpc("SendMessage", params);
  }

  getTask(taskId: string): Promise<Record<string, unknown>> {
    return this.rpc("GetTask", { id: taskId });
  }

  cancelTask(taskId: string): Promise<Record<string, unknown>> {
    return this.rpc("CancelTask", { id: taskId });
  }

  private async rpc(method: string, params: Record<string, unknown>): Promise<Record<string, unknown>> {
    this.rpcId += 1;
    const url = this.endpoint.endsWith("/a2a") ? this.endpoint : `${this.endpoint.replace(/\/$/, "")}/a2a`;
    const headers: Record<string, string> = { "content-type": "application/json" };
    if (this.apiKey) headers.authorization = `Bearer ${this.apiKey}`;
    let resp: Response;
    try {
      resp = await fetch(url, {
        method: "POST",
        headers,
        body: JSON.stringify({ jsonrpc: "2.0", id: this.rpcId, method, params }),
      });
    } catch (e) {
      throw new RuntimeConnectionError(`cannot reach A2A peer at ${url}: ${e instanceof Error ? e.message : e}`);
    }
    const body = (await resp.json()) as { result?: Record<string, unknown>; error?: { code?: unknown; message?: string; data?: unknown } };
    if (!resp.ok) throw new RuntimeRejectedError("a2a_http", JSON.stringify(body).slice(0, 200), resp.status, body.error);
    if (body.error) {
      throw new RuntimeRejectedError(String(body.error.code ?? "a2a_error"), String(body.error.message ?? "A2A error"), resp.status, body.error.data);
    }
    if (!body.result) throw new RuntimeRejectedError("a2a_error", "A2A result was not an object", resp.status);
    return body.result;
  }
}
