import { Agent } from "./agent.ts";
import { RuntimeClient } from "./client.ts";
import { CancellationError, ModelAdapterError, RuntimeConnectionError, RuntimeRejectedError, ToolError } from "./errors.ts";
import { Goal } from "./goal.ts";
import { jsonable } from "./json.ts";
import { StateProvider, StateSnapshot } from "./state.ts";
import { WORKER_CAPABILITIES } from "./version.ts";

export type WorkerIdentity = {
  workerId: string;
  language: "typescript";
  capabilities: readonly string[];
};

export class Worker {
  readonly identity: WorkerIdentity;
  requestsServed = 0;
  lastStatus: string | null = null;
  constructor(
    readonly client: RuntimeClient,
    readonly runId: string,
    readonly agent: Agent,
    readonly goal: Goal,
    readonly stateProvider: StateProvider | undefined,
    readonly pollTimeoutMs = 20_000,
    readonly signal?: AbortSignal,
  ) {
    this.identity = {
      workerId: client.workerId ?? crypto.randomUUID(),
      language: "typescript",
      capabilities: WORKER_CAPABILITIES,
    };
    client.workerId = this.identity.workerId;
    client.workerCapabilities = [...WORKER_CAPABILITIES];
  }

  async serve(): Promise<string> {
    let failures = 0;
    while (true) {
      if (this.signal?.aborted) throw new CancellationError("worker cancelled");
      let poll: Record<string, unknown> | null;
      try {
        poll = await this.client.poll(this.runId, this.pollTimeoutMs, this.signal);
        failures = 0;
      } catch (e) {
        if (e instanceof CancellationError) throw e;
        if (e instanceof RuntimeConnectionError) {
          failures += 1;
          if (failures > 40) throw e;
          const wait = Math.min(2 ** failures, 10) * 1000;
          await sleep(wait, this.signal);
          continue;
        }
        throw e;
      }
      if (!poll) continue;
      this.lastStatus = String(poll.run_status ?? this.lastStatus ?? "");
      const req = poll.request as Record<string, unknown> | undefined | null;
      if (req) await this.handle(req);
      if (poll.terminal) return this.lastStatus || "UNKNOWN";
    }
  }

  private async handle(req: Record<string, unknown>): Promise<void> {
    const requestId = String(req.request_id);
    const kind = String(req.kind ?? "");
    const payload = (req.payload as Record<string, unknown>) ?? {};
    this.requestsServed += 1;
    try {
      let result: unknown;
      if (kind === "model.generate") result = await this.modelGenerate(payload);
      else if (kind === "tool.execute") result = await this.toolExecute(payload);
      else if (kind === "state.snapshot") result = await this.stateSnapshot(payload);
      else if (kind === "goal.evaluate") result = await this.goalEvaluate(payload);
      else if (kind === "action.verify") result = await this.actionVerify(payload);
      else {
        await this.respondError(requestId, "not_implemented", `unknown request kind ${kind}`);
        return;
      }
      try {
        await this.client.respond(this.runId, requestId, { result });
      } catch (e) {
        if (!(e instanceof RuntimeRejectedError)) throw e;
      }
    } catch (e) {
      if (e instanceof ToolError) {
        await this.respondError(requestId, e.code, e.message, e.retryable, e.details);
        return;
      }
      if (e instanceof ModelAdapterError) {
        await this.respondError(requestId, e.code, e.message, e.retryable, e.details);
        return;
      }
      const err = e instanceof Error ? e : new Error(String(e));
      const retryable = err.name === "TimeoutError" || /timed out/i.test(err.message);
      await this.respondError(
        requestId,
        retryable ? "timeout" : "worker_exception",
        retryable ? `${kind} exceeded its local timeout` : `${err.name}: ${err.message}`,
        false,
      );
    }
  }

  private async respondError(requestId: string, code: string, message: string, retryable = false, details?: unknown): Promise<void> {
    try {
      await this.client.respond(this.runId, requestId, {
        error: { code, message, retryable, details: jsonable(details) ?? null },
      });
    } catch (e) {
      if (!(e instanceof RuntimeRejectedError)) throw e;
    }
  }

  private async modelGenerate(p: Record<string, unknown>): Promise<Record<string, unknown>> {
    const resp = await this.agent.adapter.generate(
      (p.messages as Record<string, unknown>[]) ?? [],
      (p.tools as Record<string, unknown>[]) ?? [],
      (p.config as Record<string, unknown>) ?? {},
    );
    return jsonable(resp) as Record<string, unknown>;
  }

  private async toolExecute(p: Record<string, unknown>): Promise<Record<string, unknown>> {
    const name = String(p.tool_name);
    let tool;
    try {
      tool = this.agent.tool(name);
    } catch {
      throw new ToolError(`tool \`${name}\` is not registered on agent \`${this.agent.name}\``, { code: "unknown_tool" });
    }
    const output = await tool.invoke((p.arguments as Record<string, unknown>) ?? {}, {
      idempotencyKey: (p.idempotency_key as string) ?? null,
      timeoutMs: Number(p.timeout_ms ?? tool.timeoutMs),
      signal: this.signal,
    });
    return { output: jsonable(output) };
  }

  private async snapshot(reason: string, step: number | null): Promise<StateSnapshot> {
    if (!this.stateProvider) throw new ToolError("no state provider registered", { code: "no_state_provider" });
    const res = await this.stateProvider.snapshot({ runId: this.runId, reason, step });
    if (!(res instanceof StateSnapshot)) {
      throw new ToolError("StateProvider.snapshot must return a StateSnapshot", { code: "invalid_snapshot" });
    }
    return res;
  }

  private async stateSnapshot(p: Record<string, unknown>): Promise<Record<string, unknown>> {
    const snap = await this.snapshot(String(p.reason ?? "unspecified"), (p.step as number | null) ?? null);
    return { state: jsonable(snap.state), version: snap.version, source: snap.source, captured_at: snap.capturedAt };
  }

  private async goalEvaluate(p: Record<string, unknown>): Promise<Record<string, unknown>> {
    const snap = new StateSnapshot({
      state: (p.state as Record<string, unknown>) ?? {},
      version: (p.state_version as string | null) ?? null,
      source: "runtime-provided",
    });
    return jsonable(await this.goal.evaluate(snap, Boolean(p.evaluate_success))) as Record<string, unknown>;
  }

  private async actionVerify(p: Record<string, unknown>): Promise<Record<string, unknown>> {
    const tool = this.agent.tool(String(p.tool_name));
    const snap = new StateSnapshot({
      state: (p.state as Record<string, unknown>) ?? {},
      version: (p.state_version as string | null) ?? null,
      source: "runtime-provided",
    });
    const res = await tool.verify((p.arguments as Record<string, unknown>) ?? {}, p.output, snap);
    return jsonable(res) as Record<string, unknown>;
  }
}

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new CancellationError());
      return;
    }
    const t = setTimeout(resolve, ms);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(t);
        reject(new CancellationError());
      },
      { once: true },
    );
  });
}
