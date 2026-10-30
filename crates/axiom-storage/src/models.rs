use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct ProjectRow {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub slug: String,
    pub retention_days: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct AgentRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: String,
    pub source_framework: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct AgentVersionRow {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub version: i32,
    pub definition: Value,
    pub config_hash: String,
    pub model: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct RunRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub agent_version_id: Option<Uuid>,
    pub agent_name: String,
    pub goal_id: Option<Uuid>,
    pub goal_description: String,
    pub workflow_id: Option<Uuid>,
    pub trace_id: String,
    pub status: String,
    pub verification_status: String,
    pub source_framework: String,
    pub model: String,
    pub input: Value,
    pub definition: Value,
    pub failure_reason: Option<String>,
    pub final_output: Option<String>,
    pub step_count: i32,
    pub next_sequence: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    pub usage_known: bool,
    pub estimated_cost_usd: Option<f64>,
    pub cost_known: bool,
    pub idempotency_key: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub lease_owner: Option<String>,
    #[serde(default)]
    pub lease_generation: i64,
    #[serde(default)]
    pub lease_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub recovery_generation: i32,
    #[serde(default)]
    pub parent_run_id: Option<Uuid>,
    #[serde(default)]
    pub task_id: Option<Uuid>,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct RunTaskRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub parent_run_id: Uuid,
    pub child_run_id: Uuid,
    pub task_id: Uuid,
    pub correlation_id: String,
    pub role: String,
    pub status: String,
    pub verification_status: String,
    pub requires_approval: bool,
    pub approval_id: Option<Uuid>,
    pub depth: i32,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct SharedContextRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub scope_id: Uuid,
    pub key: String,
    pub value: Value,
    pub version: i64,
    pub owner_agent: String,
    pub visibility: String,
    pub evidence_refs: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct SharedContextHistoryRow {
    pub id: Uuid,
    pub context_id: Uuid,
    pub project_id: Uuid,
    pub scope_id: Uuid,
    pub key: String,
    pub value: Value,
    pub version: i64,
    pub owner_agent: String,
    pub visibility: String,
    pub evidence_refs: Value,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct RunStepRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub step_number: i32,
    pub phase: String,
    pub status: String,
    pub summary: Value,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct EventRow {
    pub event_id: Uuid,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub sequence: i64,
    pub event_type: String,
    pub schema_version: String,
    pub source_timestamp: DateTime<Utc>,
    pub ingested_at: DateTime<Utc>,
    pub agent_id: Option<String>,
    pub step_id: Option<String>,
    pub source_framework: String,
    pub status: Option<String>,
    pub duration_ms: Option<i64>,
    pub payload: Value,
    pub metadata: Value,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct ToolInvocationRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub step_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub status: String,
    pub effect: String,
    pub risk: String,
    pub arguments: Value,
    pub result: Option<Value>,
    pub error: Option<Value>,
    pub idempotency_key: String,
    pub attempts: i32,
    pub requires_approval: bool,
    pub approval_id: Option<Uuid>,
    pub verification_status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub dispatch_state: String,
    #[serde(default)]
    pub input_fingerprint: Option<String>,
    #[serde(default)]
    pub external_operation_id: Option<String>,
    #[serde(default)]
    pub business_operation_status: Option<String>,
    #[serde(default)]
    pub last_provider_ack: Option<Value>,
    #[serde(default)]
    pub recovery_policy: Option<String>,
    #[serde(default)]
    pub reconciliation_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub recovery_disposition: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct StateSnapshotRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub step_id: Option<String>,
    pub reason: String,
    pub source: String,
    pub version: Option<String>,
    pub state: Value,
    pub changes: Option<Value>,
    pub captured_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct CheckpointRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub step_number: i32,
    pub status: String,
    pub data: Value,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub schema_version: i32,
    #[serde(default)]
    pub last_event_sequence: Option<i64>,
    #[serde(default)]
    pub recovery_generation: i32,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct ActionJournalRow {
    pub id: Uuid,
    pub invocation_id: Uuid,
    pub run_id: Uuid,
    pub dispatch_state: String,
    pub detail: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct RecoveryAttemptRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub attempt: i32,
    pub phase: String,
    pub reason: Option<String>,
    pub action_taken: String,
    pub lease_generation: Option<i64>,
    pub result: String,
    pub detail: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct GoalEvaluationRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub step_id: Option<String>,
    pub phase: String,
    pub success: Option<bool>,
    pub invariants: Value,
    pub evidence: Value,
    pub reason: Option<String>,
    pub state_snapshot_id: Option<Uuid>,
    pub evaluated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct ApprovalRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub tool_invocation_id: Uuid,
    pub status: String,
    pub agent_name: String,
    pub action_name: String,
    pub arguments: Value,
    pub risk: String,
    pub reason: String,
    pub requested_permissions: Value,
    pub requested_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decided_by: Option<String>,
    pub decision_reason: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct VerificationResultRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub step_id: Option<String>,
    pub subject_type: String,
    pub subject_id: String,
    pub status: String,
    pub method: String,
    pub expected: Option<Value>,
    pub actual: Option<Value>,
    pub evidence_source: Option<String>,
    pub evidence_version: Option<String>,
    pub evidence_timestamp: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct ModelUsageRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub step_id: Option<String>,
    pub model: String,
    pub provider: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub latency_ms: Option<i64>,
    pub provider_response_id: Option<String>,
    pub finish_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct IntegrationRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub framework: String,
    pub adapter_version: String,
    pub framework_version: Option<String>,
    pub status: String,
    pub capabilities: Value,
    pub unsupported_capabilities: Value,
    pub config: Value,
    pub last_event_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct WorkflowRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub version: i32,
    pub definition: Value,
    pub description: String,
    pub status: String,
    pub published_version: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct WorkflowVersionRow {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub project_id: Uuid,
    pub version: i32,
    pub definition: Value,
    pub published_at: DateTime<Utc>,
    pub published_by: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct WorkflowNodeExecutionRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub run_id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_version: i32,
    pub node_id: String,
    pub status: String,
    pub attempt: i32,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub child_run_id: Option<Uuid>,
    pub approval_id: Option<Uuid>,
    pub wait_until: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub kind: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub rotated_from: Option<Uuid>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct ProjectMemberRow {
    pub project_id: Uuid,
    pub subject: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct SecurityEventRow {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub actor: String,
    pub action: String,
    pub outcome: String,
    pub details: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct EvidenceRecordRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub source_system: String,
    pub resource_id: String,
    pub resource_version: Option<String>,
    pub retrieved_at: DateTime<Utc>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub content_fingerprint: String,
    pub trust: String,
    pub access_scope: Option<String>,
    pub inferred: bool,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct EvidenceNodeRow {
    pub id: String,
    pub run_id: Uuid,
    pub kind: String,
    pub resource_id: Option<String>,
    pub version: Option<String>,
    pub label: Option<String>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct EvidenceEdgeRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub from_id: String,
    pub to_id: String,
    pub kind: String,
    pub inferred: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct DecisionRecordRow {
    pub id: String,
    pub run_id: Uuid,
    pub kind: String,
    pub status: String,
    pub payload: Value,
    pub evidence_ids: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct ImpactReportRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub changes: Value,
    pub report: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct RevalidationResultRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub decision_id: String,
    pub previous_status: Option<String>,
    pub new_status: String,
    pub reason: Option<String>,
    pub predicate_results: Value,
    pub old_version: Option<String>,
    pub new_version: Option<String>,
    pub approval_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct CoverageReportRow {
    pub id: Uuid,
    pub run_id: Uuid,
    pub tool_name: String,
    pub policy: String,
    pub status: String,
    pub gate: String,
    pub report: Value,
    pub created_at: DateTime<Utc>,
}
