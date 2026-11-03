use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Closed list of event types for protocol schema version 1.x.
///
/// Unknown types are rejected at ingestion with a descriptive error; readers of a newer
/// minor version must ignore types they do not understand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
pub enum EventType {
    // Lifecycle
    #[serde(rename = "run.created")]
    RunCreated,
    #[serde(rename = "run.started")]
    RunStarted,
    #[serde(rename = "run.paused")]
    RunPaused,
    #[serde(rename = "run.resumed")]
    RunResumed,
    #[serde(rename = "run.completed")]
    RunCompleted,
    #[serde(rename = "run.failed")]
    RunFailed,
    #[serde(rename = "run.cancelled")]
    RunCancelled,
    #[serde(rename = "run.status_changed")]
    RunStatusChanged,
    // Model
    #[serde(rename = "model.requested")]
    ModelRequested,
    #[serde(rename = "model.stream_started")]
    ModelStreamStarted,
    #[serde(rename = "model.completed")]
    ModelCompleted,
    #[serde(rename = "model.failed")]
    ModelFailed,
    #[serde(rename = "model.usage_reported")]
    ModelUsageReported,
    // Tools
    #[serde(rename = "tool.requested")]
    ToolRequested,
    #[serde(rename = "tool.validated")]
    ToolValidated,
    #[serde(rename = "tool.started")]
    ToolStarted,
    #[serde(rename = "tool.completed")]
    ToolCompleted,
    #[serde(rename = "tool.failed")]
    ToolFailed,
    #[serde(rename = "tool.timed_out")]
    ToolTimedOut,
    // Goals
    #[serde(rename = "goal.created")]
    GoalCreated,
    #[serde(rename = "goal.evaluated")]
    GoalEvaluated,
    #[serde(rename = "goal.satisfied")]
    GoalSatisfied,
    #[serde(rename = "goal.failed")]
    GoalFailed,
    // Verification
    #[serde(rename = "verification.started")]
    VerificationStarted,
    #[serde(rename = "verification.passed")]
    VerificationPassed,
    #[serde(rename = "verification.failed")]
    VerificationFailed,
    #[serde(rename = "verification.inconclusive")]
    VerificationInconclusive,
    // State
    #[serde(rename = "state.snapshot")]
    StateSnapshot,
    #[serde(rename = "state.changed")]
    StateChanged,
    #[serde(rename = "state.conflict_detected")]
    StateConflictDetected,
    // Control
    #[serde(rename = "approval.requested")]
    ApprovalRequested,
    #[serde(rename = "approval.granted")]
    ApprovalGranted,
    #[serde(rename = "approval.denied")]
    ApprovalDenied,
    #[serde(rename = "approval.invalidated")]
    ApprovalInvalidated,
    #[serde(rename = "evidence.recorded")]
    EvidenceRecorded,
    #[serde(rename = "evidence.changed")]
    EvidenceChanged,
    #[serde(rename = "decision.created")]
    DecisionCreated,
    #[serde(rename = "decision.revalidation_required")]
    DecisionRevalidationRequired,
    #[serde(rename = "decision.revalidated")]
    DecisionRevalidated,
    #[serde(rename = "impact.computed")]
    ImpactComputed,
    #[serde(rename = "evidence.coverage_evaluated")]
    EvidenceCoverageEvaluated,
    #[serde(rename = "action.gated")]
    ActionGated,
    #[serde(rename = "recovery.started")]
    RecoveryStarted,
    #[serde(rename = "recovery.completed")]
    RecoveryCompleted,
    // Multi-agent
    #[serde(rename = "agent.delegated")]
    AgentDelegated,
    #[serde(rename = "agent.message_sent")]
    AgentMessageSent,
    #[serde(rename = "agent.message_received")]
    AgentMessageReceived,
    #[serde(rename = "agent.handoff_completed")]
    AgentHandoffCompleted,
}

impl EventType {
    /// Wire name, e.g. `tool.completed`.
    pub fn as_str(&self) -> &'static str {
        // serde_json::to_value never fails for a unit enum variant.
        match self {
            EventType::RunCreated => "run.created",
            EventType::RunStarted => "run.started",
            EventType::RunPaused => "run.paused",
            EventType::RunResumed => "run.resumed",
            EventType::RunCompleted => "run.completed",
            EventType::RunFailed => "run.failed",
            EventType::RunCancelled => "run.cancelled",
            EventType::RunStatusChanged => "run.status_changed",
            EventType::ModelRequested => "model.requested",
            EventType::ModelStreamStarted => "model.stream_started",
            EventType::ModelCompleted => "model.completed",
            EventType::ModelFailed => "model.failed",
            EventType::ModelUsageReported => "model.usage_reported",
            EventType::ToolRequested => "tool.requested",
            EventType::ToolValidated => "tool.validated",
            EventType::ToolStarted => "tool.started",
            EventType::ToolCompleted => "tool.completed",
            EventType::ToolFailed => "tool.failed",
            EventType::ToolTimedOut => "tool.timed_out",
            EventType::GoalCreated => "goal.created",
            EventType::GoalEvaluated => "goal.evaluated",
            EventType::GoalSatisfied => "goal.satisfied",
            EventType::GoalFailed => "goal.failed",
            EventType::VerificationStarted => "verification.started",
            EventType::VerificationPassed => "verification.passed",
            EventType::VerificationFailed => "verification.failed",
            EventType::VerificationInconclusive => "verification.inconclusive",
            EventType::StateSnapshot => "state.snapshot",
            EventType::StateChanged => "state.changed",
            EventType::StateConflictDetected => "state.conflict_detected",
            EventType::ApprovalRequested => "approval.requested",
            EventType::ApprovalGranted => "approval.granted",
            EventType::ApprovalDenied => "approval.denied",
            EventType::ApprovalInvalidated => "approval.invalidated",
            EventType::EvidenceRecorded => "evidence.recorded",
            EventType::EvidenceChanged => "evidence.changed",
            EventType::DecisionCreated => "decision.created",
            EventType::DecisionRevalidationRequired => "decision.revalidation_required",
            EventType::DecisionRevalidated => "decision.revalidated",
            EventType::ImpactComputed => "impact.computed",
            EventType::EvidenceCoverageEvaluated => "evidence.coverage_evaluated",
            EventType::ActionGated => "action.gated",
            EventType::RecoveryStarted => "recovery.started",
            EventType::RecoveryCompleted => "recovery.completed",
            EventType::AgentDelegated => "agent.delegated",
            EventType::AgentMessageSent => "agent.message_sent",
            EventType::AgentMessageReceived => "agent.message_received",
            EventType::AgentHandoffCompleted => "agent.handoff_completed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }

    /// All known types, used for schema export and contract tests.
    pub fn all() -> &'static [EventType] {
        use EventType::*;
        &[
            RunCreated,
            RunStarted,
            RunPaused,
            RunResumed,
            RunCompleted,
            RunFailed,
            RunCancelled,
            RunStatusChanged,
            ModelRequested,
            ModelStreamStarted,
            ModelCompleted,
            ModelFailed,
            ModelUsageReported,
            ToolRequested,
            ToolValidated,
            ToolStarted,
            ToolCompleted,
            ToolFailed,
            ToolTimedOut,
            GoalCreated,
            GoalEvaluated,
            GoalSatisfied,
            GoalFailed,
            VerificationStarted,
            VerificationPassed,
            VerificationFailed,
            VerificationInconclusive,
            StateSnapshot,
            StateChanged,
            StateConflictDetected,
            ApprovalRequested,
            ApprovalGranted,
            ApprovalDenied,
            ApprovalInvalidated,
            EvidenceRecorded,
            EvidenceChanged,
            DecisionCreated,
            DecisionRevalidationRequired,
            DecisionRevalidated,
            ImpactComputed,
            EvidenceCoverageEvaluated,
            ActionGated,
            RecoveryStarted,
            RecoveryCompleted,
            AgentDelegated,
            AgentMessageSent,
            AgentMessageReceived,
            AgentHandoffCompleted,
        ]
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Coarse status attached to an event. Interpretation depends on the event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Info,
    Requested,
    Validated,
    Started,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Passed,
    Inconclusive,
    NotEvaluated,
    Pending,
    Granted,
    Denied,
    Unverified,
}

/// The framework-neutral execution event envelope (protocol 1.0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ExecutionEvent {
    /// Protocol schema version, currently `1.0`.
    pub schema_version: String,
    /// Globally unique event identifier; deduplication key for ingestion.
    pub event_id: Uuid,
    pub run_id: Uuid,
    pub trace_id: String,
    pub span_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    /// Per-run monotonic sequence. Allocated by the runtime for native runs and by the
    /// collector (arrival order) for external runs.
    pub sequence: i64,
    pub event_type: EventType,
    /// Source timestamp (UTC). Ingestion time is stored separately by the collector.
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    /// e.g. `goshikq`, `langgraph`, `openai-agents`, `otel`.
    pub source_framework: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<EventStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl ExecutionEvent {
    /// Builder for native runtime events; sequence is assigned by the persistence layer.
    pub fn native(run_id: Uuid, trace_id: impl Into<String>, event_type: EventType) -> Self {
        ExecutionEvent {
            schema_version: crate::SCHEMA_VERSION.to_string(),
            event_id: Uuid::now_v7(),
            run_id,
            trace_id: trace_id.into(),
            span_id: new_span_id(),
            parent_span_id: None,
            sequence: 0,
            event_type,
            timestamp: Utc::now(),
            agent_id: None,
            step_id: None,
            source_framework: "goshikq".to_string(),
            status: None,
            duration_ms: None,
            payload: serde_json::Value::Object(Default::default()),
            metadata: serde_json::Value::Object(Default::default()),
        }
    }

    pub fn with_status(mut self, status: EventStatus) -> Self {
        self.status = Some(status);
        self
    }
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }
    pub fn with_step(mut self, step_id: impl Into<String>) -> Self {
        self.step_id = Some(step_id.into());
        self
    }
    pub fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }
    pub fn with_parent(mut self, parent_span_id: impl Into<String>) -> Self {
        self.parent_span_id = Some(parent_span_id.into());
        self
    }
    pub fn with_span(mut self, span_id: impl Into<String>) -> Self {
        self.span_id = span_id.into();
        self
    }
    pub fn with_duration(mut self, ms: i64) -> Self {
        self.duration_ms = Some(ms);
        self
    }
}

/// 16-byte hex span identifier compatible with W3C trace-context span ids.
pub fn new_span_id() -> String {
    let id = Uuid::new_v4();
    hex_lower(&id.as_bytes()[..8])
}

/// 32-hex-char trace id compatible with W3C trace-context.
pub fn new_trace_id() -> String {
    hex_lower(Uuid::new_v4().as_bytes())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

/// Batch ingestion request for external frameworks and SDK-side emitters.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct EventBatch {
    pub events: Vec<ExecutionEvent>,
}

/// Result of an idempotent batch ingestion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct EventBatchResult {
    pub accepted: usize,
    pub duplicates: usize,
    pub rejected: Vec<EventRejection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct EventRejection {
    pub event_id: Uuid,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_roundtrip() {
        for t in EventType::all() {
            let s = serde_json::to_string(t).unwrap();
            let back: EventType = serde_json::from_str(&s).unwrap();
            assert_eq!(*t, back);
            assert_eq!(format!("\"{}\"", t.as_str()), s);
        }
    }

    #[test]
    fn ids_are_w3c_shaped() {
        assert_eq!(new_span_id().len(), 16);
        assert_eq!(new_trace_id().len(), 32);
    }
}
