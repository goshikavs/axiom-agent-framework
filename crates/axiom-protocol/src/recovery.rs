//! Durable recovery contracts: action-journal dispatch states, versioned checkpoints,
//! crash-test barriers. IO-free. Exactly-once external I/O is not claimed.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Where a side-effecting action sits in the journal. Distinct from `ToolStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DispatchState {
    /// Intent persisted; worker has not been asked.
    Prepared,
    /// Request may have reached the provider; local response was not recorded.
    DispatchUncertain,
    /// Provider acknowledged the request.
    Accepted,
    /// Accepted; authoritative settlement is still pending.
    OutcomePending,
    /// Authoritative state confirms the business outcome.
    OutcomeConfirmed,
    /// Provider rejected, or the action was cancelled/denied.
    Rejected,
    /// Operator or reconciler must inspect; do not auto-retry.
    RecoveryRequired,
}

impl DispatchState {
    pub fn as_str(&self) -> &'static str {
        match self {
            DispatchState::Prepared => "PREPARED",
            DispatchState::DispatchUncertain => "DISPATCH_UNCERTAIN",
            DispatchState::Accepted => "ACCEPTED",
            DispatchState::OutcomePending => "OUTCOME_PENDING",
            DispatchState::OutcomeConfirmed => "OUTCOME_CONFIRMED",
            DispatchState::Rejected => "REJECTED",
            DispatchState::RecoveryRequired => "RECOVERY_REQUIRED",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }
}

/// Test-only process abort points. Wired by `AXIOM_CRASH_AT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CrashBarrier {
    BeforeDispatch,
    AfterIntent,
    DuringTool,
    AfterAccept,
    DuringVerify,
    WaitingApproval,
}

impl CrashBarrier {
    pub fn as_str(&self) -> &'static str {
        match self {
            CrashBarrier::BeforeDispatch => "before_dispatch",
            CrashBarrier::AfterIntent => "after_intent",
            CrashBarrier::DuringTool => "during_tool",
            CrashBarrier::AfterAccept => "after_accept",
            CrashBarrier::DuringVerify => "during_verify",
            CrashBarrier::WaitingApproval => "waiting_approval",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "before_dispatch" => Some(Self::BeforeDispatch),
            "after_intent" => Some(Self::AfterIntent),
            "during_tool" => Some(Self::DuringTool),
            "after_accept" => Some(Self::AfterAccept),
            "during_verify" => Some(Self::DuringVerify),
            "waiting_approval" => Some(Self::WaitingApproval),
            _ => None,
        }
    }
}

/// Versioned, serializable executor snapshot. No Python stacks or sockets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CheckpointV2 {
    pub schema_version: u32,
    pub run_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_version: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_version_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_config_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_predicate: Option<String>,
    pub phase: String,
    pub step: u32,
    pub step_count: u32,
    pub limits: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<DateTime<Utc>>,
    pub recovery_generation: i32,
    pub last_event_sequence: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_invocation_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_approval_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_deadline_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_state_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_state_snapshot_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_output: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl CheckpointV2 {
    pub const SCHEMA_VERSION: u32 = 2;
}

/// Current-state projection returned by the recovery API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RecoveryReport {
    pub run_id: Uuid,
    pub status: String,
    pub verification_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_operation_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_disposition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_generation: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_generation: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checkpoint_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checkpoint_phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_approval_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncertain_write: Option<bool>,
    pub recovery_attempts: Vec<RecoveryAttemptView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RecoveryAttemptView {
    pub attempt: i32,
    pub phase: String,
    pub action_taken: String,
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_states_round_trip() {
        for s in [
            DispatchState::Prepared,
            DispatchState::DispatchUncertain,
            DispatchState::Accepted,
            DispatchState::OutcomePending,
            DispatchState::OutcomeConfirmed,
            DispatchState::Rejected,
            DispatchState::RecoveryRequired,
        ] {
            assert_eq!(DispatchState::parse(s.as_str()), Some(s));
        }
    }

    #[test]
    fn crash_barriers_parse() {
        assert_eq!(CrashBarrier::parse("before_dispatch"), Some(CrashBarrier::BeforeDispatch));
        assert_eq!(CrashBarrier::parse("after_intent"), Some(CrashBarrier::AfterIntent));
        assert_eq!(CrashBarrier::parse("during_tool"), Some(CrashBarrier::DuringTool));
        assert_eq!(CrashBarrier::parse("after_accept"), Some(CrashBarrier::AfterAccept));
        assert_eq!(CrashBarrier::parse("during_verify"), Some(CrashBarrier::DuringVerify));
        assert_eq!(CrashBarrier::parse("waiting_approval"), Some(CrashBarrier::WaitingApproval));
        assert_eq!(CrashBarrier::parse("nope"), None);
    }
}
