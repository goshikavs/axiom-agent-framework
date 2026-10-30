import { ConfigurationError } from "./errors.ts";
import { StateSnapshot } from "./state.ts";

export type VerificationPolicy = "required" | "optional" | "none";

export type InvariantOutcome = {
  satisfied: boolean;
  reason?: string | null;
  expected?: unknown;
  actual?: unknown;
};

export type Predicate = (state: StateSnapshot) => boolean | InvariantOutcome | Promise<boolean | InvariantOutcome>;

export type GoalInit = {
  description: string;
  success?: Predicate;
  invariants?: Predicate[];
  maxSteps?: number | null;
  deadlineSeconds?: number | null;
  externalWaitSeconds?: number | null;
  maxTotalTokens?: number | null;
  maxCostUsd?: number | null;
  verification?: VerificationPolicy;
  metadata?: Record<string, unknown>;
};

function predicateName(p: Predicate): string {
  return p.name || "anonymous";
}

export class Goal {
  readonly description: string;
  readonly success?: Predicate;
  readonly invariants: Predicate[];
  readonly maxSteps?: number | null;
  readonly deadlineSeconds?: number | null;
  readonly externalWaitSeconds?: number | null;
  readonly maxTotalTokens?: number | null;
  readonly maxCostUsd?: number | null;
  readonly verification: VerificationPolicy;
  readonly metadata: Record<string, unknown>;

  constructor(init: GoalInit) {
    if (!init.description.trim()) throw new ConfigurationError("Goal.description is required");
    this.description = init.description;
    this.success = init.success;
    this.invariants = [...(init.invariants ?? [])];
    this.maxSteps = init.maxSteps;
    this.deadlineSeconds = init.deadlineSeconds;
    this.externalWaitSeconds = init.externalWaitSeconds;
    this.maxTotalTokens = init.maxTotalTokens;
    this.maxCostUsd = init.maxCostUsd;
    this.verification = init.verification ?? "required";
    this.metadata = init.metadata ?? {};
    const names = this.invariants.map(predicateName);
    if (new Set(names).size !== names.length) {
      throw new ConfigurationError(`invariant names must be unique: ${names.join(", ")}`);
    }
  }

  get successName(): string | null {
    return this.success ? predicateName(this.success) : null;
  }

  get invariantNames(): string[] {
    return this.invariants.map(predicateName);
  }

  toDefinition(): Record<string, unknown> {
    const d: Record<string, unknown> = {
      description: this.description,
      success_predicate: this.successName,
      invariants: this.invariantNames,
      verification_policy: this.verification,
      metadata: this.metadata,
    };
    if (this.maxSteps != null) d.max_steps = this.maxSteps;
    if (this.deadlineSeconds != null) d.deadline_seconds = this.deadlineSeconds;
    if (this.externalWaitSeconds != null) d.external_wait_seconds = this.externalWaitSeconds;
    if (this.maxTotalTokens != null || this.maxCostUsd != null) {
      d.budget = { max_total_tokens: this.maxTotalTokens ?? null, max_cost_usd: this.maxCostUsd ?? null };
    }
    return d;
  }

  async evaluate(snapshot: StateSnapshot, evaluateSuccess: boolean): Promise<Record<string, unknown>> {
    const invariants = [];
    for (const p of this.invariants) {
      const out = await runPredicate(p, snapshot);
      invariants.push({ name: predicateName(p), satisfied: out.satisfied, reason: out.reason ?? null });
    }
    const result: Record<string, unknown> = {
      invariants,
      evidence: { source: snapshot.source, version: snapshot.version },
    };
    if (evaluateSuccess && this.success) {
      const out = await runPredicate(this.success, snapshot);
      result.success = out.satisfied;
      result.reason = out.reason ?? null;
      (result.evidence as Record<string, unknown>).predicate = this.successName;
      (result.evidence as Record<string, unknown>).expected = out.expected ?? null;
      (result.evidence as Record<string, unknown>).actual = out.actual ?? null;
    }
    return result;
  }
}

async function runPredicate(p: Predicate, snapshot: StateSnapshot): Promise<InvariantOutcome> {
  try {
    const res = await p(snapshot);
    if (typeof res === "boolean") {
      return { satisfied: res, reason: res ? null : `${predicateName(p)} returned False` };
    }
    if (res && typeof res === "object" && "satisfied" in res) return res;
    return { satisfied: false, reason: `${predicateName(p)} returned non-boolean` };
  } catch (e) {
    const err = e instanceof Error ? e : new Error(String(e));
    return { satisfied: false, reason: `${predicateName(p)} raised ${err.name}: ${err.message}` };
  }
}
