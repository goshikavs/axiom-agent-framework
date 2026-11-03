use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Execution lifecycle status of a run. Transition rules live in `goshikq-core`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunStatus {
    Created,
    Planning,
    ValidatingAction,
    WaitingForApproval,
    Executing,
    ReconcilingState,
    VerifyingGoal,
    Replanning,
    Paused,
    RecoveryRequired,
    /// Processor/API accepted a side effect; authoritative settlement is still pending.
    /// Non-terminal. The runtime polls state without calling the model or re-dispatching the tool.
    WaitingForExternalState,
    Completed,
    /// The agent reported completion but no authoritative verification was possible
    /// (no state provider / success predicate). Distinct from `Completed` on purpose.
    Unverified,
    Failed,
    Cancelled,
    /// Wall-clock run deadline elapsed (`ExecutionLimits.deadline_seconds`).
    /// Conceptual name: `EXECUTION_TIMED_OUT`. Stored as `TIMED_OUT` (v0.1 rows).
    TimedOut,
    /// Legacy catch-all written by v0.1 when either steps or tokens/cost ran out.
    /// New runs write `STEP_LIMIT_EXCEEDED` or `MODEL_BUDGET_EXCEEDED` instead.
    /// `parse("BUDGET_EXCEEDED")` still succeeds so existing rows load.
    BudgetExceeded,
    StepLimitExceeded,
    ModelBudgetExceeded,
}

impl RunStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunStatus::Completed
                | RunStatus::Unverified
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::TimedOut
                | RunStatus::BudgetExceeded
                | RunStatus::StepLimitExceeded
                | RunStatus::ModelBudgetExceeded
        )
    }

    /// Statuses that must survive a runtime restart without being marked `RECOVERY_REQUIRED`.
    /// A worker must re-attach before snapshots or tool dispatch can continue.
    pub fn is_durable_wait(&self) -> bool {
        matches!(
            self,
            RunStatus::WaitingForApproval | RunStatus::WaitingForExternalState | RunStatus::Paused
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Created => "CREATED",
            RunStatus::Planning => "PLANNING",
            RunStatus::ValidatingAction => "VALIDATING_ACTION",
            RunStatus::WaitingForApproval => "WAITING_FOR_APPROVAL",
            RunStatus::Executing => "EXECUTING",
            RunStatus::ReconcilingState => "RECONCILING_STATE",
            RunStatus::VerifyingGoal => "VERIFYING_GOAL",
            RunStatus::Replanning => "REPLANNING",
            RunStatus::Paused => "PAUSED",
            RunStatus::RecoveryRequired => "RECOVERY_REQUIRED",
            RunStatus::WaitingForExternalState => "WAITING_FOR_EXTERNAL_STATE",
            RunStatus::Completed => "COMPLETED",
            RunStatus::Unverified => "UNVERIFIED",
            RunStatus::Failed => "FAILED",
            RunStatus::Cancelled => "CANCELLED",
            RunStatus::TimedOut => "TIMED_OUT",
            RunStatus::BudgetExceeded => "BUDGET_EXCEEDED",
            RunStatus::StepLimitExceeded => "STEP_LIMIT_EXCEEDED",
            RunStatus::ModelBudgetExceeded => "MODEL_BUDGET_EXCEEDED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "EXECUTION_TIMED_OUT" => Some(RunStatus::TimedOut),
            other => serde_json::from_value(serde_json::Value::String(other.to_string())).ok(),
        }
    }
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Outcome verification status for a goal or an action.
/// Distinct from execution status, tool status, and business-operation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationStatus {
    Verified,
    Failed,
    Inconclusive,
    NotEvaluated,
}

/// Authoritative external business-operation status. Never inferred from model text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BusinessOperationStatus {
    Unknown,
    Requested,
    Accepted,
    PendingSettlement,
    Settled,
    Rejected,
}

impl BusinessOperationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BusinessOperationStatus::Unknown => "UNKNOWN",
            BusinessOperationStatus::Requested => "REQUESTED",
            BusinessOperationStatus::Accepted => "ACCEPTED",
            BusinessOperationStatus::PendingSettlement => "PENDING_SETTLEMENT",
            BusinessOperationStatus::Settled => "SETTLED",
            BusinessOperationStatus::Rejected => "REJECTED",
        }
    }
}

/// Canonical outcome contract returned by the verification API.
/// The five fields are independent: a tool may succeed while the goal is unverified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct OutcomeContract {
    pub execution_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_operation_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_satisfied: Option<bool>,
    pub verification_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier: Option<String>,
}

impl VerificationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VerificationStatus::Verified => "VERIFIED",
            VerificationStatus::Failed => "FAILED",
            VerificationStatus::Inconclusive => "INCONCLUSIVE",
            VerificationStatus::NotEvaluated => "NOT_EVALUATED",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }
}

/// Tool invocation lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolStatus {
    Requested,
    Validated,
    ApprovalRequired,
    Approved,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    ReconciliationRequired,
    Cancelled,
}

impl ToolStatus {
    pub fn parse(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ToolStatus::Requested => "REQUESTED",
            ToolStatus::Validated => "VALIDATED",
            ToolStatus::ApprovalRequired => "APPROVAL_REQUIRED",
            ToolStatus::Approved => "APPROVED",
            ToolStatus::Running => "RUNNING",
            ToolStatus::Succeeded => "SUCCEEDED",
            ToolStatus::Failed => "FAILED",
            ToolStatus::TimedOut => "TIMED_OUT",
            ToolStatus::ReconciliationRequired => "RECONCILIATION_REQUIRED",
            ToolStatus::Cancelled => "CANCELLED",
        }
    }
}

/// Effect category of an action. Drives retry safety (see ADR-003).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema, Default,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EffectCategory {
    #[default]
    ReadOnly,
    Reversible,
    IdempotentWrite,
    NonIdempotentWrite,
    Irreversible,
}

impl EffectCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            EffectCategory::ReadOnly => "READ_ONLY",
            EffectCategory::Reversible => "REVERSIBLE",
            EffectCategory::IdempotentWrite => "IDEMPOTENT_WRITE",
            EffectCategory::NonIdempotentWrite => "NON_IDEMPOTENT_WRITE",
            EffectCategory::Irreversible => "IRREVERSIBLE",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }

    /// Whether a timed-out / ambiguous invocation may be retried without reconciliation.
    pub fn safe_to_retry_when_ambiguous(&self) -> bool {
        matches!(
            self,
            EffectCategory::ReadOnly | EffectCategory::Reversible | EffectCategory::IdempotentWrite
        )
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    #[default]
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
    /// Prerequisites changed after the grant; the token must not authorize the write.
    Invalidated,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Denied => "denied",
            ApprovalStatus::Expired => "expired",
            ApprovalStatus::Invalidated => "invalidated",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "denied" => Some(Self::Denied),
            "expired" => Some(Self::Expired),
            "invalidated" => Some(Self::Invalidated),
            _ => None,
        }
    }
}

/// Whether the goal requires authoritative verification to count as completed.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum VerificationPolicy {
    /// A run can only reach `COMPLETED` if the success predicate holds against a state
    /// snapshot. Without a state provider the run ends `UNVERIFIED`.
    #[default]
    Required,
    /// Verify when a provider exists; otherwise end `UNVERIFIED`.
    Optional,
    /// No verification attempted; the run always ends `UNVERIFIED` when the model stops.
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RetryPolicy {
    /// Maximum attempts including the first one.
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_backoff_ms")]
    pub initial_backoff_ms: u64,
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

fn default_max_attempts() -> u32 {
    2
}
fn default_backoff_ms() -> u64 {
    200
}
fn default_backoff_multiplier() -> f64 {
    2.0
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_attempts: default_max_attempts(),
            initial_backoff_ms: default_backoff_ms(),
            backoff_multiplier: default_backoff_multiplier(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ExecutionLimits {
    #[serde(default = "default_max_steps")]
    pub max_steps: u32,
    /// Wall-clock deadline for the whole run, in seconds.
    #[serde(default = "default_deadline_seconds")]
    pub deadline_seconds: u64,
    /// Total token budget (input + output) across all model calls, when usage is reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_tokens: Option<u64>,
    /// Estimated cost budget in USD, enforced only when pricing is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_usd: Option<f64>,
    /// Maximum consecutive replans after the model claims completion but the goal is unmet.
    #[serde(default = "default_max_replans")]
    pub max_replans: u32,
    /// Maximum serialized size of a tool result accepted into context, in bytes.
    #[serde(default = "default_tool_result_bytes")]
    pub max_tool_result_bytes: usize,
    /// Bound on waiting for external settlement without burning model steps.
    #[serde(default = "default_external_wait_seconds")]
    pub external_wait_seconds: u64,
}

fn default_max_steps() -> u32 {
    10
}
fn default_deadline_seconds() -> u64 {
    300
}
fn default_max_replans() -> u32 {
    2
}
fn default_tool_result_bytes() -> usize {
    64 * 1024
}
fn default_external_wait_seconds() -> u64 {
    30
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        ExecutionLimits {
            max_steps: default_max_steps(),
            deadline_seconds: default_deadline_seconds(),
            max_total_tokens: None,
            max_cost_usd: None,
            max_replans: default_max_replans(),
            max_tool_result_bytes: default_tool_result_bytes(),
            external_wait_seconds: default_external_wait_seconds(),
        }
    }
}

/// Action contract registered with the runtime for a tool exposed by the SDK worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ToolSpec {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// JSON Schema (draft 2020-12 or draft-07) for the arguments object.
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub effect: EffectCategory,
    #[serde(default)]
    pub risk: RiskLevel,
    #[serde(default = "default_tool_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    /// The tool/API honors the `idempotency_key` passed by the runtime.
    #[serde(default)]
    pub idempotent: bool,
    #[serde(default)]
    pub requires_approval: bool,
    #[serde(default)]
    pub required_permissions: Vec<String>,
    /// Names of preconditions evaluated by the worker before execution (informational;
    /// evaluated by the SDK, reported in `tool.validated`).
    #[serde(default)]
    pub preconditions: Vec<String>,
    /// The SDK registered an outcome verifier for this action.
    #[serde(default)]
    pub has_verifier: bool,
    /// How this tool is revalidated before a write. Default preserves v0.3 selective behavior.
    #[serde(default)]
    pub revalidation_policy: crate::RevalidationPolicy,
    /// Declared evidence requirements. Completeness of this list is not completeness of reality.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_coverage: Option<crate::EvidenceCoverageContract>,
}

fn default_tool_timeout_ms() -> u64 {
    30_000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct AgentDefinition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Provider-qualified model identifier, e.g. `mock/deterministic`, `openai/<model>`.
    pub model: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    #[serde(default)]
    pub execution_limits: ExecutionLimits,
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    #[serde(default)]
    pub context_policy: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Permissions granted to this agent; tools requiring other permissions are rejected.
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GoalDefinition {
    pub description: String,
    /// Name of the success predicate implemented by the worker, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_predicate: Option<String>,
    /// Names of invariants implemented by the worker.
    #[serde(default)]
    pub invariants: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_seconds: Option<u64>,
    /// Bound on waiting for external settlement (seconds). Min'd with agent limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_wait_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<GoalBudget>,
    #[serde(default)]
    pub verification_policy: VerificationPolicy,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema, Default)]
pub struct GoalBudget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_usd: Option<f64>,
}

/// Request body for `POST /api/v1/runs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CreateRunRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<uuid::Uuid>,
    pub agent: AgentDefinition,
    pub goal: GoalDefinition,
    /// Initial user input / task context handed to the model.
    #[serde(default)]
    pub input: serde_json::Value,
    /// Whether the worker can serve `state.snapshot` requests.
    #[serde(default)]
    pub has_state_provider: bool,
    /// Framework that produced this definition, e.g. `goshikq-python`.
    #[serde(default = "default_source_framework")]
    pub source_framework: String,
    /// Client-supplied key to make run creation idempotent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Persist the run without spawning an executor. Used for coordinators waiting on children.
    #[serde(default)]
    pub defer_start: bool,
    /// Parent run when this run is a delegated child task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<uuid::Uuid>,
    /// Unique id of the delegated task (distinct from `run.id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<uuid::Uuid>,
    /// Shared parent/child correlation id. Defaults to the parent run id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// When set, reuse this trace instead of minting a new one (child tasks share the parent trace).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

fn default_source_framework() -> String {
    "goshikq".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timed_out_parses_execution_timed_out_alias() {
        assert_eq!(RunStatus::parse("TIMED_OUT"), Some(RunStatus::TimedOut));
        assert_eq!(
            RunStatus::parse("EXECUTION_TIMED_OUT"),
            Some(RunStatus::TimedOut)
        );
        assert_eq!(RunStatus::TimedOut.as_str(), "TIMED_OUT");
    }

    #[test]
    fn budget_status_split_is_additive() {
        assert_eq!(
            RunStatus::parse("BUDGET_EXCEEDED"),
            Some(RunStatus::BudgetExceeded)
        );
        assert_eq!(
            RunStatus::parse("STEP_LIMIT_EXCEEDED"),
            Some(RunStatus::StepLimitExceeded)
        );
        assert_eq!(
            RunStatus::parse("MODEL_BUDGET_EXCEEDED"),
            Some(RunStatus::ModelBudgetExceeded)
        );
        assert!(RunStatus::StepLimitExceeded.is_terminal());
        assert!(RunStatus::ModelBudgetExceeded.is_terminal());
        assert!(!RunStatus::WaitingForExternalState.is_terminal());
    }

    #[test]
    fn durable_waits_are_not_terminal() {
        assert!(RunStatus::WaitingForApproval.is_durable_wait());
        assert!(RunStatus::WaitingForExternalState.is_durable_wait());
        assert!(RunStatus::Paused.is_durable_wait());
        assert!(!RunStatus::Executing.is_durable_wait());
        assert!(!RunStatus::RecoveryRequired.is_durable_wait());
    }

    #[test]
    fn outcome_contract_fields_are_independent() {
        let c = OutcomeContract {
            execution_status: "WAITING_FOR_EXTERNAL_STATE".into(),
            tool_status: Some("SUCCEEDED".into()),
            business_operation_status: Some("PENDING_SETTLEMENT".into()),
            goal_satisfied: Some(false),
            verification_status: "INCONCLUSIVE".into(),
            evidence_source: Some("simulated-orders+processor".into()),
            evidence_version: Some("1".into()),
            observed_at: None,
            verifier: Some("sdk.action_verifier".into()),
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["execution_status"], "WAITING_FOR_EXTERNAL_STATE");
        assert_eq!(v["tool_status"], "SUCCEEDED");
        assert_eq!(v["business_operation_status"], "PENDING_SETTLEMENT");
        assert_eq!(v["goal_satisfied"], false);
        assert_eq!(v["verification_status"], "INCONCLUSIVE");
    }
}
