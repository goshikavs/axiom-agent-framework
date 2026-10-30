export const TERMINAL_STATUSES = new Set([
  "COMPLETED",
  "UNVERIFIED",
  "FAILED",
  "CANCELLED",
  "TIMED_OUT",
  "BUDGET_EXCEEDED",
  "STEP_LIMIT_EXCEEDED",
  "MODEL_BUDGET_EXCEEDED",
  "REJECTED",
  "RECOVERY_REQUIRED",
]);

export type VerificationResult = {
  runId: string;
  status: string;
  verificationStatus: string;
  explanation?: string;
  goal?: unknown;
  raw: Record<string, unknown>;
};

export class ExecutionResult {
  runId: string;
  status: string;
  verificationStatus: string;
  traceId: string;
  finalOutput: string | null;
  failureReason: string | null;
  stepCount: number;
  inputTokens: number;
  outputTokens: number;
  usageKnown: boolean;
  estimatedCostUsd: number | null;
  costKnown: boolean;
  durationMs: number | null;
  studioUrl: string | null;
  toolStatus: string | null;
  dispatchState: string | null;
  businessOperationStatus: string | null;
  raw: Record<string, unknown>;

  constructor(init: {
    runId: string;
    status: string;
    verificationStatus: string;
    traceId: string;
    finalOutput?: string | null;
    failureReason?: string | null;
    stepCount?: number;
    inputTokens?: number;
    outputTokens?: number;
    usageKnown?: boolean;
    estimatedCostUsd?: number | null;
    costKnown?: boolean;
    durationMs?: number | null;
    studioUrl?: string | null;
    toolStatus?: string | null;
    dispatchState?: string | null;
    businessOperationStatus?: string | null;
    raw?: Record<string, unknown>;
  }) {
    this.runId = init.runId;
    this.status = init.status;
    this.verificationStatus = init.verificationStatus;
    this.traceId = init.traceId;
    this.finalOutput = init.finalOutput ?? null;
    this.failureReason = init.failureReason ?? null;
    this.stepCount = init.stepCount ?? 0;
    this.inputTokens = init.inputTokens ?? 0;
    this.outputTokens = init.outputTokens ?? 0;
    this.usageKnown = init.usageKnown ?? false;
    this.estimatedCostUsd = init.estimatedCostUsd ?? null;
    this.costKnown = init.costKnown ?? false;
    this.durationMs = init.durationMs ?? null;
    this.studioUrl = init.studioUrl ?? null;
    this.toolStatus = init.toolStatus ?? null;
    this.dispatchState = init.dispatchState ?? null;
    this.businessOperationStatus = init.businessOperationStatus ?? null;
    this.raw = init.raw ?? {};
  }

  get verified(): boolean {
    return this.status === "COMPLETED" && this.verificationStatus === "VERIFIED";
  }

  get succeeded(): boolean {
    return this.status === "COMPLETED";
  }

  get terminal(): boolean {
    return TERMINAL_STATUSES.has(this.status);
  }

  static fromApi(run: Record<string, unknown>, studioUrl?: string | null): ExecutionResult {
    return new ExecutionResult({
      runId: String(run.id),
      status: String(run.status ?? ""),
      verificationStatus: String(run.verification_status ?? "NOT_EVALUATED"),
      traceId: String(run.trace_id ?? ""),
      finalOutput: (run.final_output as string | null) ?? null,
      failureReason: (run.failure_reason as string | null) ?? null,
      stepCount: Number(run.step_count ?? 0),
      inputTokens: Number(run.input_tokens ?? 0),
      outputTokens: Number(run.output_tokens ?? 0),
      usageKnown: Boolean(run.usage_known),
      estimatedCostUsd: (run.estimated_cost_usd as number | null) ?? null,
      costKnown: Boolean(run.cost_known),
      durationMs: (run.duration_ms as number | null) ?? null,
      studioUrl: studioUrl ?? null,
      toolStatus: (run.tool_status as string | null) ?? null,
      dispatchState: (run.dispatch_state as string | null) ?? null,
      businessOperationStatus: (run.business_operation_status as string | null) ?? null,
      raw: run,
    });
  }

  summary(): string {
    const cost = this.costKnown && this.estimatedCostUsd != null ? `$${this.estimatedCostUsd.toFixed(6)}` : "unknown";
    return `run ${this.runId} ${this.status}/${this.verificationStatus} steps=${this.stepCount} cost=${cost}`;
  }
}
