//! API response shapes. Rows from storage are mapped here so internal columns
//! (`next_sequence`, full definitions) do not leak into list endpoints.

use chrono::{DateTime, Utc};
use goshikq_storage::*;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema)]
pub struct Paginated<T: ToSchema> {
    pub items: Vec<T>,
    pub total: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PageQuery {
    /// Max items (1-500, default 50).
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunSummary {
    pub id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub agent_version_id: Option<Uuid>,
    pub agent_name: String,
    pub goal_description: String,
    pub trace_id: String,
    pub status: String,
    pub verification_status: String,
    pub source_framework: String,
    pub model: String,
    pub failure_reason: Option<String>,
    pub step_count: i32,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    /// False when at least one model call did not report usage; token totals are then partial.
    pub usage_known: bool,
    /// Estimated; `null` when pricing is unknown for any call in the run.
    pub estimated_cost_usd: Option<f64>,
    pub cost_known: bool,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub metadata: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

impl From<RunRow> for RunSummary {
    fn from(r: RunRow) -> Self {
        RunSummary {
            id: r.id,
            project_id: r.project_id,
            agent_id: r.agent_id,
            agent_version_id: r.agent_version_id,
            agent_name: r.agent_name,
            goal_description: r.goal_description,
            trace_id: r.trace_id,
            status: r.status,
            verification_status: r.verification_status,
            source_framework: r.source_framework,
            model: r.model,
            failure_reason: r.failure_reason,
            step_count: r.step_count,
            input_tokens: r.input_tokens,
            output_tokens: r.output_tokens,
            cached_tokens: r.cached_tokens,
            usage_known: r.usage_known,
            estimated_cost_usd: if r.cost_known {
                r.estimated_cost_usd
            } else {
                None
            },
            cost_known: r.cost_known,
            created_at: r.created_at,
            started_at: r.started_at,
            ended_at: r.ended_at,
            duration_ms: r.duration_ms,
            metadata: r.metadata,
            parent_run_id: r.parent_run_id,
            task_id: r.task_id,
            correlation_id: r.correlation_id,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunDetail {
    #[serde(flatten)]
    pub summary: RunSummary,
    pub input: serde_json::Value,
    pub final_output: Option<String>,
    /// The full `CreateRunRequest` as submitted (agent + goal contracts).
    pub definition: serde_json::Value,
    pub event_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dispatch_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub business_operation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_disposition: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunEvidenceResponse {
    pub run_id: Uuid,
    pub records: Vec<goshikq_storage::EvidenceRecordRow>,
    pub nodes: Vec<goshikq_storage::EvidenceNodeRow>,
    pub edges: Vec<goshikq_storage::EvidenceEdgeRow>,
    pub decisions: Vec<goshikq_storage::DecisionRecordRow>,
    pub impact: Vec<goshikq_storage::ImpactReportRow>,
    pub revalidation: Vec<goshikq_storage::RevalidationResultRow>,
    #[serde(default)]
    pub coverage: Vec<goshikq_storage::CoverageReportRow>,
    /// Edges are stored relationships, not inferred LLM traces.
    pub inferred: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateRunResponse {
    pub run_id: Uuid,
    pub status: String,
    pub trace_id: String,
    /// True when an idempotency key matched an existing run.
    pub reused: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunVerificationReport {
    pub run_id: Uuid,
    pub status: String,
    pub verification_status: String,
    pub goal: serde_json::Value,
    pub intended_outcome: String,
    pub final_output: Option<String>,
    pub goal_evaluations: Vec<GoalEvaluationRow>,
    pub verification_results: Vec<VerificationResultRow>,
    pub state_snapshots: Vec<StateSnapshotRow>,
    pub tool_invocations: Vec<ToolInvocationRow>,
    /// Human-readable explanation of why the run has its verification status.
    pub explanation: String,
    /// Canonical five-way outcome contract. Fields are independent.
    pub outcome: goshikq_protocol::OutcomeContract,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunStateResponse {
    pub run_id: Uuid,
    pub snapshots: Vec<StateSnapshotRow>,
    pub latest: Option<StateSnapshotRow>,
    pub checkpoint: Option<CheckpointRow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunMetricsResponse {
    pub run_id: Uuid,
    pub duration_ms: Option<i64>,
    pub step_count: i32,
    pub model_calls: i64,
    pub tool_calls: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub usage_known: bool,
    pub estimated_cost_usd: Option<f64>,
    pub cost_known: bool,
    pub model_latency_ms_total: Option<i64>,
    pub tool_duration_ms_total: Option<i64>,
    /// duration - (model latency + tool duration); only meaningful when both are known.
    pub framework_overhead_ms: Option<i64>,
    pub model_usage: Vec<ModelUsageRow>,
    pub steps: Vec<RunStepRow>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct RunListQuery {
    pub status: Option<String>,
    pub verification_status: Option<String>,
    pub agent: Option<String>,
    pub framework: Option<String>,
    pub model: Option<String>,
    pub search: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub project: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct EventListQuery {
    /// Return events with sequence greater than this value.
    pub after: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct StreamQuery {
    /// Resume after this sequence (alternative to `Last-Event-ID`).
    pub after: Option<i64>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct RecentEventsQuery {
    pub event_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct WorkerPollQuery {
    /// Long-poll wait in milliseconds (max 30000).
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CancelRunRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ApprovalListQuery {
    /// `pending` | `approved` | `denied` | `expired`
    pub status: Option<String>,
    pub run_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ApprovalDecisionRequest {
    /// `approve` or `deny`
    pub decision: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApprovalDecisionResponse {
    pub approval: ApprovalRow,
    /// False if the decision had already been recorded (idempotent replay).
    pub applied: bool,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MetricsQuery {
    pub agent: Option<String>,
    pub framework: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    /// `minute` | `hour` | `day` | `week`
    pub bucket: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OverviewResponse {
    pub totals: goshikq_storage::metrics::OverviewTotals,
    pub pending_approvals: i64,
    pub series: Vec<goshikq_storage::metrics::TimeBucket>,
    pub verification: Vec<goshikq_storage::metrics::VerificationBreakdown>,
    pub models: Vec<goshikq_storage::metrics::ModelBreakdown>,
    pub filters: FilterValues,
    pub by_framework: Vec<goshikq_storage::metrics::GroupBreakdown>,
    pub pricing_version: String,
    pub bucket: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FilterValues {
    pub agents: Vec<String>,
    pub frameworks: Vec<String>,
    pub models: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CostResponse {
    pub pricing_version: String,
    pub totals: goshikq_storage::metrics::OverviewTotals,
    pub by_model: Vec<goshikq_storage::metrics::ModelBreakdown>,
    pub by_agent: Vec<goshikq_storage::metrics::GroupBreakdown>,
    pub by_framework: Vec<goshikq_storage::metrics::GroupBreakdown>,
    pub series: Vec<goshikq_storage::metrics::TimeBucket>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveAgentRequest {
    pub agent: goshikq_protocol::AgentDefinition,
    #[serde(default = "default_fw")]
    pub source_framework: String,
}

fn default_fw() -> String {
    "goshikq".into()
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentDetail {
    pub agent: AgentRow,
    pub versions: Vec<AgentVersionRow>,
    pub run_count: i64,
    pub verified_runs: i64,
    pub failed_runs: i64,
    pub last_run_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertIntegrationRequest {
    pub framework: String,
    pub adapter_version: String,
    pub framework_version: Option<String>,
    /// `connected` | `configured` | `error` | `planned`
    pub status: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub unsupported_capabilities: Vec<String>,
    #[serde(default)]
    pub config: serde_json::Value,
    pub last_error: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWorkflowRequest {
    pub definition: goshikq_core::workflow::WorkflowDefinition,
    /// When true, incomplete graphs may be stored. Publish and run still require a valid graph.
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateWorkflowRequest {
    pub definition: goshikq_core::workflow::WorkflowDefinition,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DuplicateWorkflowRequest {
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RunWorkflowRequest {
    #[serde(default)]
    pub version: Option<i32>,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkflowValidation {
    pub valid: bool,
    pub issues: Vec<goshikq_core::workflow::WorkflowIssue>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelsResponse {
    pub pricing: goshikq_core::pricing::PricingTable,
    pub capture_model_io: String,
    /// Adapter capability manifest shipped with the SDKs (config/model-adapters.json).
    pub adapters: serde_json::Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: String,
    /// Only honored when `GOSHIKQ_AUTH_MODE=none` (local bootstrap / isolation tests).
    pub project_id: Option<Uuid>,
    pub role: Option<String>,
    pub kind: Option<String>,
    pub expires_in_seconds: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertMemberRequest {
    pub subject: String,
    pub role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WhoAmI {
    pub subject: String,
    pub actor: String,
    pub role: String,
    pub kind: String,
    pub project_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub auth_mode: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyCreated {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    /// Plaintext shown once. Store it; the server keeps only a hash.
    pub token: String,
    pub created_at: DateTime<Utc>,
    pub role: String,
    pub kind: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyPublic {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub role: String,
    pub kind: String,
    pub expires_at: Option<DateTime<Utc>>,
}

impl From<ApiKeyRow> for ApiKeyPublic {
    fn from(r: ApiKeyRow) -> Self {
        ApiKeyPublic {
            id: r.id,
            project_id: r.project_id,
            name: r.name,
            created_at: r.created_at,
            last_used_at: r.last_used_at,
            revoked_at: r.revoked_at,
            role: r.role,
            kind: r.kind,
            expires_at: r.expires_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReadyResponse {
    pub status: String,
    pub database: String,
    pub active_runs: usize,
    pub auth_mode: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateTaskResponse {
    pub parent_run_id: Uuid,
    pub child_run_id: Uuid,
    pub task_id: Uuid,
    pub correlation_id: String,
    pub status: String,
    pub verification_status: String,
    pub reused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunHierarchy {
    pub run: RunSummary,
    pub parent: Option<RunSummary>,
    pub children: Vec<ChildTaskView>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChildTaskView {
    pub task: goshikq_storage::RunTaskRow,
    pub run: RunSummary,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RetentionPurgeRequest {
    /// When omitted or true, candidates are returned and nothing is deleted.
    pub dry_run: Option<bool>,
    /// Terminal runs whose `ended_at` is older than this many days. Defaults to `GOSHIKQ_RETENTION_DAYS`.
    pub older_than_days: Option<i32>,
    /// Optional allowlist. When set, only these ids are considered.
    pub only_run_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RetentionPurgeResponse {
    pub dry_run: bool,
    pub older_than: DateTime<Utc>,
    pub deleted: u64,
    pub run_ids: Vec<Uuid>,
    pub skipped_active_parents: u64,
}
