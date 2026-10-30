//! Native multi-agent contracts: groups, tasks, delegation, shared context.
//!
//! Child task completion is not parent goal verification. Remote A2A SUCCESS is not
//! an Axiom `VERIFIED` outcome.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{AgentDefinition, GoalDefinition};

/// How a coordinator schedules child tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationMode {
    Sequential,
    Parallel,
    #[default]
    Supervisor,
}

/// Failure handling after a child ends unsuccessfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    #[default]
    FailFast,
    Partial,
}

/// Bounds and scheduling for an `AgentGroup`. Stored on the parent run's metadata
/// and enforced by the runtime when creating child tasks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CoordinationPolicy {
    #[serde(default)]
    pub mode: CoordinationMode,
    /// Maximum parent→child nesting, inclusive of the first child (default 4).
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Maximum children created from one parent (default 16).
    #[serde(default = "default_max_delegations")]
    pub max_delegations: u32,
    /// Reject a child whose agent name already appears in the ancestor chain.
    #[serde(default = "default_true")]
    pub prevent_cycles: bool,
    #[serde(default)]
    pub failure_mode: FailureMode,
    /// Bounded retries of a failed child task (not of uncertain external writes).
    #[serde(default)]
    pub max_child_retries: u32,
    /// Parent goal is verified independently of child execution status.
    #[serde(default = "default_true")]
    pub require_parent_verification: bool,
}

fn default_max_depth() -> u32 {
    4
}
fn default_max_delegations() -> u32 {
    16
}
fn default_true() -> bool {
    true
}

impl Default for CoordinationPolicy {
    fn default() -> Self {
        CoordinationPolicy {
            mode: CoordinationMode::Supervisor,
            max_depth: default_max_depth(),
            max_delegations: default_max_delegations(),
            prevent_cycles: true,
            failure_mode: FailureMode::FailFast,
            max_child_retries: 0,
            require_parent_verification: true,
        }
    }
}

/// Request body for `POST /api/v1/runs/{id}/tasks`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CreateTaskRequest {
    pub agent: AgentDefinition,
    pub goal: GoalDefinition,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub has_state_provider: bool,
    #[serde(default = "default_source_framework")]
    pub source_framework: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default = "default_worker_role")]
    pub role: String,
    /// Hold the child in `CREATED` until a human approval is granted.
    #[serde(default)]
    pub requires_approval: bool,
    /// Do not spawn the child executor (SDK will call `/start` later, or approval will).
    #[serde(default)]
    pub defer_start: bool,
}

fn default_worker_role() -> String {
    "worker".into()
}
fn default_source_framework() -> String {
    "axiom".into()
}

/// Request body for `POST /api/v1/runs/{id}/handoff`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct HandoffRequest {
    pub agent: AgentDefinition,
    pub goal: GoalDefinition,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub has_state_provider: bool,
    #[serde(default = "default_source_framework")]
    pub source_framework: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// When true, cancel the handing-off run after the successor is created.
    #[serde(default)]
    pub cancel_source: bool,
}

/// Optimistic-concurrency write of a shared-context key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct SharedContextPut {
    pub key: String,
    pub value: serde_json::Value,
    /// When set, the write fails with conflict unless the stored version matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<i64>,
    #[serde(default)]
    pub owner_agent: String,
    #[serde(default = "default_visibility")]
    pub visibility: String,
    /// Resource ids in the existing evidence graph this value depends on.
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

fn default_visibility() -> String {
    "team".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct SharedContextEntry {
    pub id: Uuid,
    pub project_id: Uuid,
    pub scope_id: Uuid,
    pub key: String,
    pub value: serde_json::Value,
    pub version: i64,
    pub owner_agent: String,
    pub visibility: String,
    pub evidence_refs: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// A2A protocol version this runtime speaks (major.minor, no patch).
pub const A2A_PROTOCOL_VERSION: &str = "1.0";

/// JSON-RPC binding path for A2A (`POST /a2a`).
pub const A2A_JSONRPC_PATH: &str = "/a2a";
