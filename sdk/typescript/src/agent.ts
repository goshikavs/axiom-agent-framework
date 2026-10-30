import { ConfigurationError } from "./errors.ts";
import { type ModelAdapter, resolveModel } from "./models.ts";
import { Tool } from "./tool.ts";

export type RetryPolicy = {
  max_attempts: number;
  initial_backoff_ms: number;
  backoff_multiplier: number;
};

export type ExecutionLimits = {
  max_steps: number;
  deadline_seconds: number;
  max_total_tokens?: number | null;
  max_cost_usd?: number | null;
  max_replans: number;
  max_tool_result_bytes: number;
  external_wait_seconds: number;
};

const DEFAULT_LIMITS: ExecutionLimits = {
  max_steps: 10,
  deadline_seconds: 300,
  max_replans: 2,
  max_tool_result_bytes: 64 * 1024,
  external_wait_seconds: 30,
};

const DEFAULT_RETRY: RetryPolicy = { max_attempts: 2, initial_backoff_ms: 200, backoff_multiplier: 2 };

export type AgentInit = {
  name: string;
  model: string | ModelAdapter;
  instructions?: string;
  description?: string;
  tools?: Tool<any, any>[];
  executionLimits?: Partial<ExecutionLimits>;
  retryPolicy?: Partial<RetryPolicy>;
  contextPolicy?: Record<string, unknown>;
  permissions?: string[];
  metadata?: Record<string, unknown>;
  id?: string;
};

export class Agent {
  readonly name: string;
  readonly instructions: string;
  readonly description: string;
  readonly tools: Tool<any, any>[];
  readonly executionLimits: ExecutionLimits;
  readonly retryPolicy: RetryPolicy;
  readonly contextPolicy: Record<string, unknown>;
  readonly permissions: string[];
  readonly metadata: Record<string, unknown>;
  readonly id?: string;
  readonly adapter: ModelAdapter;

  constructor(init: AgentInit) {
    if (!init.name.trim()) throw new ConfigurationError("Agent.name is required");
    this.name = init.name;
    this.instructions = init.instructions ?? "";
    this.description = init.description ?? "";
    this.tools = [...(init.tools ?? [])];
    this.executionLimits = { ...DEFAULT_LIMITS, ...init.executionLimits };
    this.retryPolicy = { ...DEFAULT_RETRY, ...init.retryPolicy };
    this.contextPolicy = init.contextPolicy ?? {};
    this.permissions = [...(init.permissions ?? [])];
    this.metadata = init.metadata ?? {};
    this.id = init.id;
    const names = this.tools.map((t) => t.name);
    if (new Set(names).size !== names.length) {
      throw new ConfigurationError(`tool names must be unique: ${names.join(", ")}`);
    }
    this.adapter = resolveModel(init.model);
  }

  get modelId(): string {
    return this.adapter.modelId;
  }

  tool(name: string): Tool<any, any> {
    const found = this.tools.find((t) => t.name === name);
    if (!found) throw new ConfigurationError(`unknown tool \`${name}\``);
    return found;
  }

  toDefinition(): {
    name: string;
    description: string;
    model: string;
    instructions: string;
    tools: Record<string, unknown>[];
    execution_limits: Record<string, unknown>;
    retry_policy: RetryPolicy;
    context_policy: Record<string, unknown>;
    permissions: string[];
    metadata: Record<string, unknown>;
  } {
    const limits: Record<string, unknown> = {
      max_steps: this.executionLimits.max_steps,
      deadline_seconds: this.executionLimits.deadline_seconds,
      max_replans: this.executionLimits.max_replans,
      max_tool_result_bytes: this.executionLimits.max_tool_result_bytes,
      external_wait_seconds: this.executionLimits.external_wait_seconds,
    };
    if (this.executionLimits.max_total_tokens != null) limits.max_total_tokens = this.executionLimits.max_total_tokens;
    if (this.executionLimits.max_cost_usd != null) limits.max_cost_usd = this.executionLimits.max_cost_usd;
    return {
      name: this.name,
      description: this.description,
      model: this.modelId,
      instructions: this.instructions,
      tools: this.tools.map((t) => t.toSpec()),
      execution_limits: limits,
      retry_policy: this.retryPolicy,
      context_policy: this.contextPolicy,
      permissions: [...this.permissions],
      metadata: this.id ? { ...this.metadata, id: this.id } : { ...this.metadata },
    };
  }
}
