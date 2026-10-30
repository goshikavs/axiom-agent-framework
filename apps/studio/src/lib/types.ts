export type RunSummary = {
  id: string;
  project_id: string;
  agent_name: string;
  goal_description: string;
  trace_id: string;
  status: string;
  verification_status: string;
  source_framework: string;
  model: string;
  failure_reason: string | null;
  step_count: number;
  input_tokens: number;
  output_tokens: number;
  cached_tokens: number;
  usage_known: boolean;
  estimated_cost_usd: number | null;
  cost_known: boolean;
  created_at: string;
  started_at: string | null;
  ended_at: string | null;
  duration_ms: number | null;
  parent_run_id?: string | null;
  task_id?: string | null;
  correlation_id?: string | null;
};

export type ChildTaskView = {
  task: {
    task_id: string;
    child_run_id: string;
    role: string;
    status: string;
    verification_status: string;
    depth: number;
    requires_approval: boolean;
    correlation_id: string;
  };
  run: RunSummary;
};

export type RunHierarchy = {
  run: RunSummary;
  parent: RunSummary | null;
  children: ChildTaskView[];
};

export type RunDetail = RunSummary & {
  input: unknown;
  final_output: string | null;
  definition: unknown;
  event_count: number;
  tool_status?: string | null;
  dispatch_state?: string | null;
  business_operation_status?: string | null;
  recovery_disposition?: string | null;
};

export type EvidenceRecord = {
  id: string;
  run_id: string;
  source_system: string;
  resource_id: string;
  resource_version: string | null;
  retrieved_at: string;
  content_fingerprint: string;
  trust: string;
  access_scope: string | null;
  inferred: boolean;
};

export type EvidenceNode = {
  id: string;
  kind: string;
  resource_id: string | null;
  version: string | null;
  label: string | null;
};

export type EvidenceEdge = {
  id: string;
  from_id: string;
  to_id: string;
  kind: string;
  inferred: boolean;
};

export type DecisionRecord = {
  id: string;
  kind: string;
  status: string;
  payload: unknown;
  evidence_ids: unknown;
};

export type ImpactReportRow = {
  id: string;
  changes: unknown;
  report: unknown;
  created_at: string;
};

export type RevalidationResultRow = {
  id: string;
  decision_id: string;
  previous_status: string | null;
  new_status: string;
  reason: string | null;
  predicate_results: unknown;
  old_version: string | null;
  new_version: string | null;
  approval_id: string | null;
  created_at: string;
};

export type CoverageReportRow = {
  id: string;
  tool_name: string;
  policy: string;
  status: string;
  gate: string;
  report: unknown;
  created_at: string;
};

export type RunEvidenceResponse = {
  run_id: string;
  records: EvidenceRecord[];
  nodes: EvidenceNode[];
  edges: EvidenceEdge[];
  decisions: DecisionRecord[];
  impact: ImpactReportRow[];
  revalidation: RevalidationResultRow[];
  coverage?: CoverageReportRow[];
  inferred: boolean;
};

export type ExecutionEvent = {
  schema_version: string;
  event_id: string;
  run_id: string;
  trace_id: string;
  span_id: string;
  parent_span_id?: string | null;
  sequence: number;
  event_type: string;
  timestamp: string;
  agent_id?: string | null;
  step_id?: string | null;
  source_framework: string;
  status?: string | null;
  duration_ms?: number | null;
  payload: Record<string, unknown>;
  metadata: Record<string, unknown>;
  ingested_at?: string;
};

export type OverviewTotals = {
  total_runs: number;
  active_runs: number;
  completed_runs: number;
  unverified_runs: number;
  failed_runs: number;
  cancelled_runs: number;
  verified_runs: number;
  verification_failed_runs: number;
  inconclusive_runs?: number;
  terminal_runs: number;
  avg_duration_ms: number | null;
  p50_duration_ms: number | null;
  p95_duration_ms: number | null;
  p99_duration_ms: number | null;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cached_tokens: number;
  usage_known_runs: number;
  total_estimated_cost_usd: number | null;
  cost_known_runs: number;
  verified_runs_with_cost: number;
  verified_cost_usd: number | null;
  verification_overhead_ms?: number | null;
  verification_results_with_duration?: number;
};

export type TimeBucket = {
  bucket: string;
  total: number;
  completed: number;
  unverified: number;
  failed: number;
  verified: number;
  estimated_cost_usd: number | null;
  cost_known_runs: number;
  p50_duration_ms: number | null;
  p95_duration_ms: number | null;
  p99_duration_ms: number | null;
};

export type OverviewResponse = {
  totals: OverviewTotals;
  pending_approvals: number;
  series: TimeBucket[];
  verification: { verification_status: string; runs: number }[];
  models: {
    model: string;
    calls: number;
    input_tokens: number | null;
    output_tokens: number | null;
    avg_latency_ms: number | null;
    estimated_cost_usd: number | null;
    priced_calls: number;
  }[];
  filters: { agents: string[]; frameworks: string[]; models: string[] };
  by_framework?: { key: string; runs: number; verified: number; failed: number; estimated_cost_usd: number | null; cost_known_runs: number; input_tokens: number; output_tokens: number; usage_known_runs: number; avg_duration_ms: number | null }[];
  pricing_version: string;
  bucket: string;
};

export type Paginated<T> = {
  items: T[];
  total: number | null;
  limit: number;
  offset: number;
};

export type ApprovalRow = {
  id: string;
  run_id: string;
  status: string;
  agent_name: string;
  action_name: string;
  arguments: unknown;
  risk: string;
  reason: string;
  requested_at: string;
  expires_at: string;
  decided_at: string | null;
  decided_by: string | null;
  decision_reason: string | null;
};

export type RecoveryReport = {
  run_id: string;
  status: string;
  verification_status: string;
  tool_status?: string | null;
  dispatch_state?: string | null;
  business_operation_status?: string | null;
  recovery_disposition?: string | null;
  lease_generation?: number | null;
  lease_owner?: string | null;
  recovery_generation?: number | null;
  last_checkpoint_at?: string | null;
  last_checkpoint_phase?: string | null;
  pending_approval_id?: string | null;
  external_operation_id?: string | null;
  uncertain_write?: boolean | null;
  warning?: string | null;
  recovery_attempts: {
    attempt: number;
    phase: string;
    action_taken: string;
    result: string;
    reason?: string | null;
    created_at: string;
  }[];
};

export type VerificationReport = {
  run_id: string;
  status: string;
  verification_status: string;
  intended_outcome: string;
  final_output: string | null;
  explanation: string;
  goal: unknown;
  goal_evaluations: unknown[];
  verification_results: {
    id: string;
    subject_type: string;
    subject_id: string;
    status: string;
    method: string;
    expected: unknown;
    actual: unknown;
    evidence_source: string | null;
    evidence_version: string | null;
    evidence_timestamp: string | null;
    duration_ms: number | null;
    reason: string | null;
  }[];
  state_snapshots: {
    id: string;
    reason: string;
    source: string;
    version: string | null;
    state: unknown;
    captured_at: string;
  }[];
  tool_invocations: unknown[];
  outcome: {
    execution_status: string;
    tool_status?: string | null;
    business_operation_status?: string | null;
    goal_satisfied?: boolean | null;
    verification_status: string;
    evidence_source?: string | null;
    evidence_version?: string | null;
    observed_at?: string | null;
    verifier?: string | null;
  };
};
