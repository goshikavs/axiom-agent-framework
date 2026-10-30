import { Agent } from "./agent.ts";
import { Goal } from "./goal.ts";
import { ExecutionResult } from "./result.ts";
import { Runtime, type RuntimeOptions } from "./runtime.ts";
import { StateProvider } from "./state.ts";
import { SOURCE_FRAMEWORK } from "./version.ts";

export type CoordinationMode = "sequential" | "parallel" | "supervisor";
export type FailureMode = "fail_fast" | "partial";

export type CoordinationPolicy = {
  mode: CoordinationMode;
  max_depth: number;
  max_delegations: number;
  prevent_cycles: boolean;
  failure_mode: FailureMode;
  max_child_retries: number;
  require_parent_verification: boolean;
};

export const DEFAULT_COORDINATION: CoordinationPolicy = {
  mode: "supervisor",
  max_depth: 4,
  max_delegations: 16,
  prevent_cycles: true,
  failure_mode: "fail_fast",
  max_child_retries: 0,
  require_parent_verification: true,
};

export type AgentTaskInit = {
  agent: Agent;
  goal: Goal;
  input?: Record<string, unknown>;
  role?: string;
  requiresApproval?: boolean;
  stateProvider?: StateProvider;
  idempotencyKey?: string;
  metadata?: Record<string, unknown>;
};

export class AgentTask {
  constructor(readonly init: AgentTaskInit) {}
}

export type GroupResult = {
  parent: ExecutionResult;
  children: ExecutionResult[];
  hierarchy: Record<string, unknown>;
};

export class AgentGroup {
  readonly policy: CoordinationPolicy;
  readonly parentRuntime: Runtime;

  constructor(
    readonly supervisor: Agent,
    readonly opts: RuntimeOptions & { policy?: Partial<CoordinationPolicy> } = {},
  ) {
    this.policy = { ...DEFAULT_COORDINATION, ...(opts.policy ?? {}) };
    this.parentRuntime = new Runtime(supervisor, opts);
  }

  async run(parentGoal: Goal, tasks: AgentTask[], input: Record<string, unknown> = {}): Promise<GroupResult> {
    if (tasks.length > this.policy.max_delegations) {
      throw new Error(`${tasks.length} tasks exceed max_delegations=${this.policy.max_delegations}`);
    }
    const created = await this.parentRuntime.createDeferred(parentGoal, {
      input,
      metadata: { coordination: this.policy, role: "supervisor" },
    });
    const parentId = String(created.run_id);
    const children: ExecutionResult[] = [];

    const one = async (task: AgentTask): Promise<ExecutionResult> => {
      const sp = task.init.stateProvider ?? this.opts.stateProvider;
      const body: Record<string, unknown> = {
        agent: task.init.agent.toDefinition(),
        goal: task.init.goal.toDefinition(),
        input: task.init.input ?? {},
        has_state_provider: sp != null,
        source_framework: SOURCE_FRAMEWORK,
        role: task.init.role ?? "worker",
        requires_approval: task.init.requiresApproval ?? false,
        metadata: task.init.metadata ?? {},
      };
      if (task.init.idempotencyKey) body.idempotency_key = task.init.idempotencyKey;
      const child = await this.parentRuntime.client.createTask(parentId, body);
      const childId = String(child.child_run_id);
      if (child.approval_id) {
        return this.parentRuntime.result(childId);
      }
      const rt = new Runtime(task.init.agent, { ...this.opts, stateProvider: sp });
      return rt.attach(childId, task.init.goal);
    };

    if (this.policy.mode === "parallel") {
      const settled = await Promise.allSettled(tasks.map(one));
      for (const s of settled) {
        if (s.status === "fulfilled") children.push(s.value);
        else if (this.policy.failure_mode === "fail_fast") throw s.reason;
      }
    } else {
      for (const t of tasks) {
        const r = await one(t);
        children.push(r);
        if (!r.succeeded && this.policy.failure_mode === "fail_fast") break;
      }
    }

    await this.parentRuntime.client.startRun(parentId);
    const parent = await this.parentRuntime.attach(parentId, parentGoal);
    const hierarchy = await this.parentRuntime.client.hierarchy(parentId);
    return { parent, children, hierarchy };
  }
}
