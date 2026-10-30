export type Effect =
  | "READ_ONLY"
  | "REVERSIBLE"
  | "IDEMPOTENT_WRITE"
  | "NON_IDEMPOTENT_WRITE"
  | "IRREVERSIBLE";

export type ToolDefinition<I = any, O = any> = {
  name: string;
  description?: string;
  effect?: Effect;
  schema: { parse: (input: unknown) => I };
  jsonSchema: Record<string, unknown>;
  execute: (input: I) => Promise<O> | O;
};

export type RunResult = {
  run_id: string;
  status: string;
  verification_status: string;
  trace_id: string;
  final_output: string | null;
  failure_reason: string | null;
};

export type TrustClassification = "authoritative" | "derived" | "untrusted" | "inaccessible";

export type EvidenceHint = {
  id?: string;
  source_system: string;
  resource_id: string;
  resource_version?: string;
  etag?: string;
  fingerprint?: string;
  trust?: TrustClassification;
  access_scope?: string;
};

export type DecisionHint = {
  id: string;
  kind?: string;
  depends_on?: string[];
  payload?: Record<string, unknown>;
  action?: string;
};
