//! Worker protocol: typed requests the runtime sends to the SDK process that owns the
//! model adapter, tools, state provider, and goal predicates (see ADR-001).

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::run::{RunStatus, VerificationStatus};

/// Normalized chat message exchanged with model adapters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ModelMessage {
    /// `system` | `user` | `assistant` | `tool`
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// For `tool` role messages: the id of the tool call being answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ModelMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::simple("system", content)
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self::simple("user", content)
    }
    pub fn assistant(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        ModelMessage {
            role: "assistant".into(),
            content,
            tool_calls,
            tool_call_id: None,
            name: None,
        }
    }
    pub fn tool(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        ModelMessage {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: vec![],
            tool_call_id: Some(tool_call_id.into()),
            name: Some(name.into()),
        }
    }
    fn simple(role: &str, content: impl Into<String>) -> Self {
        ModelMessage {
            role: role.into(),
            content: Some(content.into()),
            tool_calls: vec![],
            tool_call_id: None,
            name: None,
        }
    }
}

/// A tool call proposed by the model. Treated as an untrusted suggestion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct TokenUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Tool schema as presented to the model (name/description/parameters only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ModelToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ModelGenerateRequest {
    pub model: String,
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ModelToolSchema>,
    #[serde(default)]
    pub config: serde_json::Value,
    pub step: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ModelGenerateResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// `None` when the provider did not report usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,
    /// Model identifier actually used (may differ from requested, e.g. provider alias).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ToolExecuteRequest {
    pub invocation_id: Uuid,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    /// Stable key for this logical action; identical across retries of the same call.
    pub idempotency_key: String,
    pub attempt: u32,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ToolExecuteResult {
    pub output: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct StateSnapshotRequest {
    /// Why the snapshot is taken: `initial`, `post_action`, `pre_approved_action`, `reconcile`, `final`.
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct StateSnapshotResult {
    pub state: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Identifier of the authoritative source (e.g. `orders-db`, `payments-api`).
    #[serde(default)]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GoalEvaluateRequest {
    pub state: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_version: Option<String>,
    /// `initial` (invariants only), `progress`, or `final`.
    pub phase: String,
    pub evaluate_success: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct InvariantResult {
    pub name: String,
    pub satisfied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GoalEvaluateResult {
    /// `None` when no success predicate was evaluated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(default)]
    pub invariants: Vec<InvariantResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Structured evidence: expected/actual values and their source.
    #[serde(default)]
    pub evidence: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ActionVerifyRequest {
    pub invocation_id: Uuid,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    pub state: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ActionVerifyResult {
    pub status: VerificationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// When true the write was accepted but settlement is pending; do not re-dispatch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wait: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_operation_status: Option<crate::BusinessOperationStatus>,
}

/// Typed request kinds, serialized as `{"kind": "...", "payload": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(tag = "kind", content = "payload")]
pub enum WorkerRequestKind {
    #[serde(rename = "model.generate")]
    ModelGenerate(ModelGenerateRequest),
    #[serde(rename = "tool.execute")]
    ToolExecute(ToolExecuteRequest),
    #[serde(rename = "state.snapshot")]
    StateSnapshot(StateSnapshotRequest),
    #[serde(rename = "goal.evaluate")]
    GoalEvaluate(GoalEvaluateRequest),
    #[serde(rename = "action.verify")]
    ActionVerify(ActionVerifyRequest),
}

impl WorkerRequestKind {
    pub fn name(&self) -> &'static str {
        match self {
            WorkerRequestKind::ModelGenerate(_) => "model.generate",
            WorkerRequestKind::ToolExecute(_) => "tool.execute",
            WorkerRequestKind::StateSnapshot(_) => "state.snapshot",
            WorkerRequestKind::GoalEvaluate(_) => "goal.evaluate",
            WorkerRequestKind::ActionVerify(_) => "action.verify",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkerRequest {
    pub request_id: Uuid,
    pub run_id: Uuid,
    #[serde(flatten)]
    pub kind: WorkerRequestKind,
    pub issued_at: DateTime<Utc>,
    /// Deadline after which the runtime stops waiting for this request.
    pub timeout_ms: u64,
}

/// Response to `POST /runs/{id}/worker/poll`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkerPollResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<WorkerRequest>,
    pub run_status: RunStatus,
    pub terminal: bool,
}

/// Structured error returned by a worker for any request kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkerError {
    /// Stable machine-readable code, e.g. `tool_error`, `timeout`, `validation_error`,
    /// `provider_error`, `connection_error`, `not_implemented`.
    pub code: String,
    pub message: String,
    /// The worker asserts the operation did not take effect and may be retried.
    #[serde(default)]
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Body of `POST /runs/{id}/worker/respond`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct WorkerResponse {
    pub request_id: Uuid,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<WorkerError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_kind_is_tagged() {
        let req = WorkerRequestKind::StateSnapshot(StateSnapshotRequest {
            reason: "initial".into(),
            step: Some(1),
        });
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["kind"], "state.snapshot");
        assert_eq!(v["payload"]["reason"], "initial");
    }
}
