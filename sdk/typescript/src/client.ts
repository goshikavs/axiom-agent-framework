import { RuntimeConnectionError, RuntimeRejectedError } from "./errors.ts";
import { VERSION } from "./version.ts";

export const DEFAULT_ENDPOINT = "http://127.0.0.1:7400";

export type RuntimeClientOptions = {
  endpoint?: string;
  apiKey?: string;
  actor?: string;
  timeoutMs?: number;
  workerId?: string;
  workerCapabilities?: string[];
};

type RequestOpts = {
  query?: Record<string, string | number | undefined>;
  json?: unknown;
  body?: BodyInit;
  headers?: Record<string, string>;
  timeoutMs?: number;
  signal?: AbortSignal;
};

export class RuntimeClient {
  readonly endpoint: string;
  readonly apiKey?: string;
  readonly actor?: string;
  readonly timeoutMs: number;
  workerId?: string;
  workerCapabilities: string[];

  constructor(opts: RuntimeClientOptions = {}) {
    this.endpoint = (opts.endpoint || env("AXIOM_RUNTIME_URL") || DEFAULT_ENDPOINT).replace(/\/$/, "");
    this.apiKey = opts.apiKey || env("AXIOM_API_KEY");
    this.actor = opts.actor;
    this.timeoutMs = opts.timeoutMs ?? 60_000;
    this.workerId = opts.workerId;
    this.workerCapabilities = opts.workerCapabilities ?? [];
  }

  private headers(extra?: Record<string, string>): Record<string, string> {
    const h: Record<string, string> = {
      accept: "application/json",
      "user-agent": `axiom-typescript/${VERSION}`,
    };
    if (this.apiKey) h.authorization = `Bearer ${this.apiKey}`;
    if (this.actor) h["x-axiom-actor"] = this.actor;
    if (this.workerId) h["x-axiom-worker-id"] = this.workerId;
    if (this.workerCapabilities.length) h["x-axiom-worker-capabilities"] = this.workerCapabilities.join(",");
    return { ...h, ...extra };
  }

  async request(method: string, path: string, opts: RequestOpts = {}): Promise<unknown> {
    const url = new URL(path.startsWith("http") ? path : `${this.endpoint}${path}`);
    for (const [k, v] of Object.entries(opts.query ?? {})) {
      if (v != null) url.searchParams.set(k, String(v));
    }
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), opts.timeoutMs ?? this.timeoutMs);
    const onOuter = () => ctrl.abort();
    opts.signal?.addEventListener("abort", onOuter, { once: true });
    try {
      const init: RequestInit = {
        method,
        headers: this.headers(opts.headers),
        signal: ctrl.signal,
      };
      if (opts.json !== undefined) {
        init.body = JSON.stringify(opts.json);
        (init.headers as Record<string, string>)["content-type"] = "application/json";
      } else if (opts.body !== undefined) {
        init.body = opts.body;
      }
      const resp = await fetch(url, init);
      const text = await resp.text();
      const body = text ? safeJson(text) : null;
      if (resp.status >= 500) {
        throw new RuntimeConnectionError(`runtime error ${resp.status} on ${method} ${path}`, { statusCode: resp.status, body });
      }
      if (resp.status >= 400) {
        const err = (body && typeof body === "object" ? (body as { error?: Record<string, unknown> }).error : null) ?? {};
        throw new RuntimeRejectedError(
          String(err.code ?? "error"),
          String(err.message ?? text.slice(0, 200)),
          resp.status,
          err.details,
        );
      }
      if (resp.status === 204 || !text) return null;
      return body;
    } catch (e) {
      if (e instanceof RuntimeConnectionError || e instanceof RuntimeRejectedError) throw e;
      const msg = e instanceof Error ? e.message : String(e);
      throw new RuntimeConnectionError(`cannot reach Axiom runtime at ${this.endpoint}: ${msg}`);
    } finally {
      clearTimeout(timer);
      opts.signal?.removeEventListener("abort", onOuter);
    }
  }

  health(): Promise<Record<string, unknown>> {
    return this.request("GET", "/health") as Promise<Record<string, unknown>>;
  }

  ready(): Promise<Record<string, unknown>> {
    return this.request("GET", "/ready") as Promise<Record<string, unknown>>;
  }

  createRun(body: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.request("POST", "/api/v1/runs", { json: body }) as Promise<Record<string, unknown>>;
  }

  getRun(runId: string): Promise<Record<string, unknown>> {
    return this.request("GET", `/api/v1/runs/${runId}`) as Promise<Record<string, unknown>>;
  }

  listRuns(params: Record<string, string | number | undefined> = {}): Promise<Record<string, unknown>> {
    return this.request("GET", "/api/v1/runs", { query: params }) as Promise<Record<string, unknown>>;
  }

  cancelRun(runId: string, reason?: string): Promise<Record<string, unknown>> {
    return this.request("POST", `/api/v1/runs/${runId}/cancel`, { json: { reason } }) as Promise<Record<string, unknown>>;
  }

  startRun(runId: string): Promise<Record<string, unknown>> {
    return this.request("POST", `/api/v1/runs/${runId}/start`) as Promise<Record<string, unknown>>;
  }

  createTask(parentRunId: string, body: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.request("POST", `/api/v1/runs/${parentRunId}/tasks`, { json: body }) as Promise<Record<string, unknown>>;
  }

  listTasks(parentRunId: string): Promise<unknown> {
    return this.request("GET", `/api/v1/runs/${parentRunId}/tasks`);
  }

  hierarchy(runId: string): Promise<Record<string, unknown>> {
    return this.request("GET", `/api/v1/runs/${runId}/hierarchy`) as Promise<Record<string, unknown>>;
  }

  handoff(runId: string, body: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.request("POST", `/api/v1/runs/${runId}/handoff`, { json: body }) as Promise<Record<string, unknown>>;
  }

  putSharedContext(runId: string, body: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.request("PUT", `/api/v1/runs/${runId}/shared-context`, { json: body }) as Promise<Record<string, unknown>>;
  }

  listSharedContext(runId: string): Promise<unknown> {
    return this.request("GET", `/api/v1/runs/${runId}/shared-context`);
  }

  listEvents(runId: string, after?: number, limit = 500): Promise<Record<string, unknown>> {
    return this.request("GET", `/api/v1/runs/${runId}/events`, {
      query: { limit, after },
    }) as Promise<Record<string, unknown>>;
  }

  getVerification(runId: string): Promise<Record<string, unknown>> {
    return this.request("GET", `/api/v1/runs/${runId}/verification`) as Promise<Record<string, unknown>>;
  }

  getState(runId: string): Promise<Record<string, unknown>> {
    return this.request("GET", `/api/v1/runs/${runId}/state`) as Promise<Record<string, unknown>>;
  }

  getRecovery(runId: string): Promise<Record<string, unknown>> {
    return this.request("GET", `/api/v1/runs/${runId}/recovery`) as Promise<Record<string, unknown>>;
  }

  poll(runId: string, timeoutMs = 20_000, signal?: AbortSignal): Promise<Record<string, unknown> | null> {
    return this.request("POST", `/api/v1/runs/${runId}/worker/poll`, {
      query: { timeout_ms: timeoutMs },
      timeoutMs: timeoutMs + 15_000,
      signal,
    }) as Promise<Record<string, unknown> | null>;
  }

  async respond(runId: string, requestId: string, opts: { result?: unknown; error?: Record<string, unknown> }): Promise<void> {
    const body: Record<string, unknown> = { request_id: requestId, ok: !opts.error };
    if (opts.error) body.error = opts.error;
    else body.result = opts.result ?? null;
    await this.request("POST", `/api/v1/runs/${runId}/worker/respond`, { json: body });
  }

  ingestOtlpJson(payload: unknown): Promise<unknown> {
    return this.request("POST", "/v1/traces", { json: payload });
  }
}

function env(name: string): string | undefined {
  try {
    const v = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env?.[name];
    return v && v.length ? v : undefined;
  } catch {
    return undefined;
  }
}

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

/** @deprecated Use RuntimeClient */
export { RuntimeClient as default };
