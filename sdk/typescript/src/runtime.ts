import { Agent } from "./agent.ts";
import { RuntimeClient } from "./client.ts";
import { CancellationError } from "./errors.ts";
import { Goal } from "./goal.ts";
import { ExecutionResult, TERMINAL_STATUSES, type VerificationResult } from "./result.ts";
import { StateProvider } from "./state.ts";
import { PROTOCOL_VERSION, SOURCE_FRAMEWORK, VERSION, WORKER_CAPABILITIES } from "./version.ts";
import { Worker } from "./worker.ts";

export type RuntimeOptions = {
  endpoint?: string;
  apiKey?: string;
  stateProvider?: StateProvider;
  projectId?: string;
  studioUrl?: string;
  pollTimeoutMs?: number;
};

export type RunOptions = {
  input?: Record<string, unknown>;
  idempotencyKey?: string;
  metadata?: Record<string, unknown>;
  signal?: AbortSignal;
  timeoutMs?: number;
};

export class Runtime {
  readonly agent: Agent;
  readonly stateProvider?: StateProvider;
  readonly projectId?: string;
  readonly studioUrl?: string;
  readonly pollTimeoutMs: number;
  readonly client: RuntimeClient;
  lastWorker: Worker | null = null;

  constructor(agent: Agent, opts: RuntimeOptions = {}) {
    this.agent = agent;
    this.stateProvider = opts.stateProvider;
    this.projectId = opts.projectId || env("GOSHIKQ_PROJECT_ID", "AXIOM_PROJECT_ID");
    this.studioUrl = (opts.studioUrl || env("GOSHIKQ_STUDIO_URL", "AXIOM_STUDIO_URL") || "").replace(/\/$/, "") || undefined;
    this.pollTimeoutMs = opts.pollTimeoutMs ?? 20_000;
    this.client = new RuntimeClient({
      endpoint: opts.endpoint,
      apiKey: opts.apiKey,
      actor: `${SOURCE_FRAMEWORK}/${VERSION}`,
    });
  }

  async run(goal: Goal, opts: RunOptions = {}): Promise<ExecutionResult> {
    const workerId = crypto.randomUUID();
    this.client.workerId = workerId;
    this.client.workerCapabilities = [...WORKER_CAPABILITIES];
    const body: Record<string, unknown> = {
      agent: this.agent.toDefinition(),
      goal: goal.toDefinition(),
      input: opts.input ?? {},
      has_state_provider: this.stateProvider != null,
      source_framework: SOURCE_FRAMEWORK,
      metadata: {
        sdk_version: VERSION,
        protocol_version: PROTOCOL_VERSION,
        worker: {
          identity: workerId,
          language: "typescript",
          capabilities: [...WORKER_CAPABILITIES],
        },
        ...(opts.metadata ?? {}),
      },
    };
    if (this.projectId) body.project_id = this.projectId;
    if (opts.idempotencyKey) body.idempotency_key = opts.idempotencyKey;

    const created = await this.client.createRun(body);
    const runId = String(created.run_id);
    const status = String(created.status ?? "");
    const deadline = opts.timeoutMs ? Date.now() + opts.timeoutMs : null;
    const signal = combineSignals(opts.signal, deadline);

    if (!created.reused || !TERMINAL_STATUSES.has(status)) {
      return this.attach(runId, goal, { signal });
    }
    return this.result(runId);
  }

  async createDeferred(goal: Goal, opts: RunOptions = {}): Promise<Record<string, unknown>> {
    const body: Record<string, unknown> = {
      agent: this.agent.toDefinition(),
      goal: goal.toDefinition(),
      input: opts.input ?? {},
      has_state_provider: this.stateProvider != null,
      source_framework: SOURCE_FRAMEWORK,
      defer_start: true,
      metadata: { sdk_version: VERSION, protocol_version: PROTOCOL_VERSION, ...(opts.metadata ?? {}) },
    };
    if (this.projectId) body.project_id = this.projectId;
    if (opts.idempotencyKey) body.idempotency_key = opts.idempotencyKey;
    return this.client.createRun(body);
  }

  async attach(runId: string, goal: Goal, opts: { signal?: AbortSignal } = {}): Promise<ExecutionResult> {
    const worker = new Worker(this.client, runId, this.agent, goal, this.stateProvider, this.pollTimeoutMs, opts.signal);
    this.lastWorker = worker;
    try {
      await worker.serve();
    } catch (e) {
      if (e instanceof CancellationError) {
        await this.client.cancelRun(runId, "client cancelled");
      }
      throw e;
    }
    return this.result(runId);
  }

  async result(runId: string): Promise<ExecutionResult> {
    const run = await this.client.getRun(runId);
    return ExecutionResult.fromApi(run, this.studioLink(runId));
  }

  async verification(runId: string): Promise<VerificationResult> {
    const raw = await this.client.getVerification(runId);
    return {
      runId,
      status: String(raw.status ?? ""),
      verificationStatus: String(raw.verification_status ?? "NOT_EVALUATED"),
      explanation: raw.explanation as string | undefined,
      goal: raw.goal,
      raw,
    };
  }

  async events(runId: string, after?: number): Promise<unknown[]> {
    const body = await this.client.listEvents(runId, after);
    return ((body as { items?: unknown[] }).items ?? []) as unknown[];
  }

  async cancel(runId: string, reason?: string): Promise<Record<string, unknown>> {
    return this.client.cancelRun(runId, reason);
  }

  async checkConnection(): Promise<Record<string, unknown>> {
    return this.client.ready();
  }

  private studioLink(runId: string): string | undefined {
    return this.studioUrl ? `${this.studioUrl}/executions/${runId}` : undefined;
  }
}

function env(...names: string[]): string | undefined {
  try {
    const proc = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env;
    for (const name of names) {
      const v = proc?.[name];
      if (v && v.length) return v;
    }
    return undefined;
  } catch {
    return undefined;
  }
}

function combineSignals(outer?: AbortSignal, deadlineMs?: number | null): AbortSignal | undefined {
  if (!outer && deadlineMs == null) return undefined;
  const ctrl = new AbortController();
  const abort = () => ctrl.abort();
  if (outer) {
    if (outer.aborted) abort();
    else outer.addEventListener("abort", abort, { once: true });
  }
  if (deadlineMs != null) {
    const wait = Math.max(0, deadlineMs - Date.now());
    setTimeout(abort, wait);
  }
  return ctrl.signal;
}
