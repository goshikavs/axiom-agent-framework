import { ConfigurationError, ModelAdapterError } from "./errors.ts";
import { jsonable } from "./json.ts";

export type Capabilities = {
  streaming: boolean;
  toolCalling: boolean;
  structuredOutput: boolean;
  multimodalInput: boolean;
  responseUsage: boolean;
  promptCaching: boolean;
  reasoningControls: boolean;
};

export const MOCK_CAPABILITIES: Capabilities = {
  streaming: false,
  toolCalling: true,
  structuredOutput: true,
  multimodalInput: false,
  responseUsage: true,
  promptCaching: false,
  reasoningControls: false,
};

export type ToolCall = {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
};

export type TokenUsage = {
  input_tokens: number;
  output_tokens: number;
  cached_tokens?: number | null;
};

export type ModelResponse = {
  content: string | null;
  tool_calls: ToolCall[];
  usage?: TokenUsage | null;
  finish_reason?: string | null;
  provider_response_id?: string | null;
  latency_ms?: number | null;
  model?: string | null;
  provider?: string | null;
};

export interface ModelAdapter {
  readonly modelId: string;
  readonly provider: string;
  readonly capabilities: Capabilities;
  generate(messages: Record<string, unknown>[], tools: Record<string, unknown>[], config: Record<string, unknown>): Promise<ModelResponse>;
}

export type ModelTurn = {
  content?: string | null;
  tool_calls?: Array<[string, Record<string, unknown>]>;
  finish_reason?: string | null;
  usage?: TokenUsage | null;
  fail?: ModelAdapterError | null;
  delay_ms?: number;
};

export type ModelPolicy = (
  messages: Record<string, unknown>[],
  tools: Record<string, unknown>[],
  turnIndex: number,
) => ModelTurn | Promise<ModelTurn>;

function syntheticUsage(messages: Record<string, unknown>[], turn: ModelTurn): TokenUsage {
  const promptChars = messages.reduce((n, m) => n + JSON.stringify(m).length, 0);
  const outChars =
    (turn.content ?? "").length + (turn.tool_calls ?? []).reduce((n, [name, args]) => n + name.length + JSON.stringify(args).length, 0);
  return { input_tokens: Math.max(1, Math.floor(promptChars / 4)), output_tokens: Math.max(1, Math.floor(outChars / 4)), cached_tokens: 0 };
}

export class MockModel implements ModelAdapter {
  readonly provider = "mock";
  readonly capabilities = MOCK_CAPABILITIES;
  readonly modelId: string;
  private script: ModelTurn[];
  private policy: ModelPolicy | null;
  private turn = 0;
  private callCounter = 0;

  constructor(opts: { script?: ModelTurn[]; policy?: ModelPolicy; modelId?: string }) {
    if ((opts.script == null) === (opts.policy == null)) {
      throw new ConfigurationError("MockModel requires exactly one of script or policy");
    }
    const modelId = opts.modelId ?? "mock/deterministic";
    if (!modelId.startsWith("mock/")) {
      throw new ConfigurationError("MockModel ids must start with `mock/`");
    }
    this.modelId = modelId;
    this.script = opts.script ? [...opts.script] : [];
    this.policy = opts.policy ?? null;
  }

  private async nextTurn(messages: Record<string, unknown>[], tools: Record<string, unknown>[]): Promise<ModelTurn> {
    const idx = this.turn++;
    if (this.policy) return await this.policy(messages, tools, idx);
    if (!this.script.length) return { content: "(empty script)" };
    return this.script[Math.min(idx, this.script.length - 1)]!;
  }

  async generate(messages: Record<string, unknown>[], tools: Record<string, unknown>[], _config: Record<string, unknown>): Promise<ModelResponse> {
    const turn = await this.nextTurn(messages, tools);
    if (turn.delay_ms) await new Promise((r) => setTimeout(r, turn.delay_ms));
    if (turn.fail) throw turn.fail;
    const calls: ToolCall[] = [];
    for (const [name, args] of turn.tool_calls ?? []) {
      this.callCounter += 1;
      calls.push({ id: `call_${this.callCounter}`, name, arguments: args });
    }
    return {
      content: turn.content ?? null,
      tool_calls: calls,
      usage: turn.usage ?? syntheticUsage(messages, turn),
      finish_reason: turn.finish_reason ?? (calls.length ? "tool_calls" : "stop"),
      provider_response_id: `mock-${this.turn}`,
      latency_ms: turn.delay_ms ?? 0,
      model: this.modelId,
      provider: this.provider,
    };
  }
}

export function lastToolResult(messages: Record<string, unknown>[], toolName?: string): Record<string, unknown> | null {
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i];
    if (!m || m.role !== "tool") continue;
    if (toolName && m.name !== toolName) continue;
    const raw = m.content;
    if (typeof raw !== "string") return asObj(raw);
    try {
      const parsed = JSON.parse(raw) as unknown;
      return asObj(parsed);
    } catch {
      return { _raw: raw };
    }
  }
  return null;
}

function asObj(value: unknown): Record<string, unknown> {
  if (value && typeof value === "object" && !Array.isArray(value)) return value as Record<string, unknown>;
  return { _value: jsonable(value) };
}

export function resolveModel(model: string | ModelAdapter): ModelAdapter {
  if (typeof model !== "string") return model;
  if (!model.includes("/")) {
    throw new ConfigurationError(`agent.model must be provider-qualified (e.g. mock/deterministic), got \`${model}\``);
  }
  if (model.startsWith("mock/")) {
    return new MockModel({ script: [{ content: `(unscripted ${model})` }], modelId: model });
  }
  throw new ConfigurationError(
    `TypeScript SDK has no live provider adapter for \`${model}\`. Use MockModel for offline runs, or execute through the Python SDK for OpenAI/Anthropic.`,
  );
}
