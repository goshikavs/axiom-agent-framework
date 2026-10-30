import type { ZodType } from "zod";
import { ConfigurationError, SchemaValidationError, ToolError } from "./errors.ts";
import { jsonable } from "./json.ts";
import type { StateSnapshot } from "./state.ts";

export type Effect =
  | "READ_ONLY"
  | "REVERSIBLE"
  | "IDEMPOTENT_WRITE"
  | "NON_IDEMPOTENT_WRITE"
  | "IRREVERSIBLE";

export type Risk = "low" | "medium" | "high" | "critical";

export type ActionContract = {
  effect: Effect;
  risk: Risk;
  idempotent: boolean;
  requiresApproval: boolean;
  permissions: string[];
  timeoutMs: number;
  maxAttempts: number;
};

export type VerifierResult = {
  status: "VERIFIED" | "FAILED" | "INCONCLUSIVE";
  expected?: unknown;
  actual?: unknown;
  reason?: string | null;
  method?: string | null;
  wait?: boolean;
  poll_after_ms?: number | null;
  business_operation_status?: string | null;
};

export const Verifier = {
  verified(opts: Partial<VerifierResult> = {}): VerifierResult {
    return { status: "VERIFIED", ...opts };
  },
  failed(reason: string, opts: Partial<VerifierResult> = {}): VerifierResult {
    return { status: "FAILED", reason, ...opts };
  },
  inconclusive(reason: string, opts: Partial<VerifierResult> = {}): VerifierResult {
    return { status: "INCONCLUSIVE", reason, ...opts };
  },
  waiting(reason: string, opts: Partial<VerifierResult> = {}): VerifierResult {
    return {
      status: "INCONCLUSIVE",
      reason,
      wait: true,
      poll_after_ms: opts.poll_after_ms ?? 500,
      business_operation_status: opts.business_operation_status ?? "PENDING_SETTLEMENT",
      ...opts,
    };
  },
};

export type ToolContext = {
  idempotencyKey: string | null;
  timeoutMs: number;
  signal?: AbortSignal;
};

export type ToolVerifier = (
  arguments_: Record<string, unknown>,
  output: unknown,
  state: StateSnapshot,
) => VerifierResult | Promise<VerifierResult>;

export type DefineToolOptions<I, O> = {
  name: string;
  description?: string;
  schema: ZodType<I>;
  jsonSchema?: Record<string, unknown>;
  outputSchema?: Record<string, unknown>;
  execute: (input: I, ctx: ToolContext) => O | Promise<O>;
  effect?: Effect | string;
  risk?: Risk | string;
  timeoutMs?: number;
  maxAttempts?: number;
  idempotent?: boolean;
  requiresApproval?: boolean;
  permissions?: string[];
  preconditions?: string[];
  verifier?: ToolVerifier;
  requiredEvidence?: string[];
  requiredInvariants?: string[];
  onMissingEvidence?: string;
  revalidationPolicy?: string;
};

function parseEffect(value: Effect | string | undefined): Effect {
  const raw = (value ?? "READ_ONLY").toString().trim().toUpperCase().replace(/-/g, "_");
  const aliases: Record<string, Effect> = {
    READ_ONLY: "READ_ONLY",
    REVERSIBLE: "REVERSIBLE",
    IDEMPOTENT_WRITE: "IDEMPOTENT_WRITE",
    NON_IDEMPOTENT_WRITE: "NON_IDEMPOTENT_WRITE",
    IRREVERSIBLE: "IRREVERSIBLE",
    UNCERTAIN_WRITE: "NON_IDEMPOTENT_WRITE",
  };
  const mapped = aliases[raw];
  if (!mapped) throw new ConfigurationError(`unknown effect \`${value}\``);
  return mapped;
}

function parseRisk(value: Risk | string | undefined): Risk {
  const raw = (value ?? "low").toString().trim().toLowerCase();
  if (raw === "low" || raw === "medium" || raw === "high" || raw === "critical") return raw;
  throw new ConfigurationError(`unknown risk \`${value}\``);
}

function requiredItem(item: string): Record<string, unknown> {
  if (item.includes(":") || item.includes("{")) {
    return { key: item, resource_id: item, require_version: true, min_trust: "authoritative" };
  }
  const [family, rest] = item.split(".", 2);
  const out: Record<string, unknown> = {
    key: item,
    resource_family: family || null,
    require_version: true,
    min_trust: "authoritative",
  };
  if (rest === "authority" || rest === "authorization" || rest === "approval") out.authorization = true;
  if (rest) out.verification_method = rest;
  return out;
}

function jsonSchemaOf<I>(schema: ZodType<I>, explicit?: Record<string, unknown>): Record<string, unknown> {
  if (explicit) return explicit;
  const maybe = schema as ZodType<I> & { toJSONSchema?: () => Record<string, unknown> };
  if (typeof maybe.toJSONSchema === "function") {
    const s = maybe.toJSONSchema();
    delete s.title;
    s.additionalProperties = false;
    s.type = s.type ?? "object";
    return s;
  }
  return { type: "object", additionalProperties: true };
}

export class Tool<I = unknown, O = unknown> {
  readonly name: string;
  readonly description: string;
  readonly schema: ZodType<I>;
  readonly jsonSchema: Record<string, unknown>;
  readonly outputSchema?: Record<string, unknown>;
  readonly executeFn: (input: I, ctx: ToolContext) => O | Promise<O>;
  readonly effect: Effect;
  readonly risk: Risk;
  readonly timeoutMs: number;
  readonly maxAttempts: number;
  readonly idempotent: boolean;
  readonly requiresApproval: boolean;
  readonly permissions: string[];
  readonly preconditions: string[];
  readonly verifier?: ToolVerifier;
  readonly revalidationPolicy: string;
  readonly evidenceCoverage?: Record<string, unknown>;

  constructor(opts: DefineToolOptions<I, O>) {
    if (!/^[A-Za-z0-9_-]+$/.test(opts.name)) {
      throw new ConfigurationError(`tool name \`${opts.name}\` must be alphanumeric with _ or -`);
    }
    this.name = opts.name;
    this.description = (opts.description ?? "").trim();
    this.schema = opts.schema;
    this.jsonSchema = jsonSchemaOf(opts.schema, opts.jsonSchema);
    this.outputSchema = opts.outputSchema;
    this.executeFn = opts.execute;
    this.effect = parseEffect(opts.effect);
    this.risk = parseRisk(opts.risk);
    this.timeoutMs = opts.timeoutMs ?? 30_000;
    this.maxAttempts = opts.maxAttempts ?? 2;
    this.idempotent = opts.idempotent ?? false;
    this.requiresApproval = opts.requiresApproval ?? false;
    this.permissions = [...(opts.permissions ?? [])];
    this.preconditions = [...(opts.preconditions ?? [])];
    this.verifier = opts.verifier;
    if (this.idempotent && this.effect === "NON_IDEMPOTENT_WRITE") {
      throw new ConfigurationError(`tool \`${this.name}\`: idempotent=true contradicts effect NON_IDEMPOTENT_WRITE`);
    }
    const coverage =
      opts.requiredEvidence?.length || opts.requiredInvariants?.length
        ? {
            required: (opts.requiredEvidence ?? []).map(requiredItem),
            required_invariants: [...(opts.requiredInvariants ?? [])],
            on_missing: opts.onMissingEvidence ?? "block",
            risk: this.risk,
          }
        : undefined;
    this.evidenceCoverage = coverage;
    this.revalidationPolicy = (opts.revalidationPolicy ?? (coverage ? "STRICT_CONTRACT" : "SELECTIVE")).trim().toUpperCase();
  }

  get contract(): ActionContract {
    return {
      effect: this.effect,
      risk: this.risk,
      idempotent: this.idempotent,
      requiresApproval: this.requiresApproval,
      permissions: [...this.permissions],
      timeoutMs: this.timeoutMs,
      maxAttempts: this.maxAttempts,
    };
  }

  toSpec(): Record<string, unknown> {
    const spec: Record<string, unknown> = {
      name: this.name,
      description: this.description,
      input_schema: this.jsonSchema,
      effect: this.effect,
      risk: this.risk,
      timeout_ms: this.timeoutMs,
      retry_policy: { max_attempts: this.maxAttempts },
      idempotent: this.idempotent,
      requires_approval: this.requiresApproval,
      required_permissions: [...this.permissions],
      preconditions: [...this.preconditions],
      has_verifier: this.verifier != null,
      revalidation_policy: this.revalidationPolicy,
    };
    if (this.outputSchema) spec.output_schema = this.outputSchema;
    if (this.evidenceCoverage) spec.evidence_coverage = this.evidenceCoverage;
    return spec;
  }

  validate(arguments_: Record<string, unknown>): I {
    const parsed = this.schema.safeParse(arguments_);
    if (!parsed.success) {
      throw new SchemaValidationError(`invalid arguments for \`${this.name}\`: ${parsed.error.message}`, parsed.error.flatten());
    }
    return parsed.data;
  }

  async invoke(arguments_: Record<string, unknown>, ctx: ToolContext): Promise<unknown> {
    const input = this.validate(arguments_);
    const timeout = ctx.timeoutMs || this.timeoutMs;
    const work = Promise.resolve(this.executeFn(input, { ...ctx, timeoutMs: timeout }));
    const timed = await withTimeout(work, timeout, ctx.signal);
    return jsonable(timed);
  }

  async verify(arguments_: Record<string, unknown>, output: unknown, state: StateSnapshot): Promise<VerifierResult> {
    if (!this.verifier) return Verifier.inconclusive("no verifier registered");
    return await this.verifier(arguments_, output, state);
  }
}

export function defineTool<I, O>(opts: DefineToolOptions<I, O>): Tool<I, O> {
  return new Tool(opts);
}

async function withTimeout<T>(work: Promise<T>, timeoutMs: number, signal?: AbortSignal): Promise<T> {
  if (signal?.aborted) throw new ToolError("cancelled", { code: "cancelled", retryable: false });
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new ToolError(`tool timed out after ${timeoutMs}ms`, { code: "timeout", retryable: true })), timeoutMs);
  });
  const abort = signal
    ? new Promise<never>((_, reject) => {
        const onAbort = () => reject(new ToolError("cancelled", { code: "cancelled", retryable: false }));
        if (signal.aborted) onAbort();
        else signal.addEventListener("abort", onAbort, { once: true });
      })
    : null;
  try {
    return await Promise.race(abort ? [work, timeout, abort] : [work, timeout]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}
