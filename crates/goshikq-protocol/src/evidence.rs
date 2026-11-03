//! Versioned evidence, decisions, and typed dependency edges.
//!
//! Explicitly registered only. Goshikq does not infer what an LLM internally considered.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// How much the runtime should trust an evidence record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrustClassification {
    /// Retrieved from the declared source of truth.
    Authoritative,
    /// Derived from authoritative evidence (a decision, a computed field).
    Derived,
    /// Operator or test injected; not used to authorize writes.
    Untrusted,
    /// The source could not be read; dependents become inconclusive.
    Inaccessible,
}

impl TrustClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustClassification::Authoritative => "authoritative",
            TrustClassification::Derived => "derived",
            TrustClassification::Untrusted => "untrusted",
            TrustClassification::Inaccessible => "inaccessible",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Source,
    Evidence,
    Decision,
    Action,
    GoalPredicate,
    Invariant,
    Approval,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Source => "source",
            NodeKind::Evidence => "evidence",
            NodeKind::Decision => "decision",
            NodeKind::Action => "action",
            NodeKind::GoalPredicate => "goal_predicate",
            NodeKind::Invariant => "invariant",
            NodeKind::Approval => "approval",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "source" => Some(Self::Source),
            "evidence" => Some(Self::Evidence),
            "decision" => Some(Self::Decision),
            "action" => Some(Self::Action),
            "goal_predicate" => Some(Self::GoalPredicate),
            "invariant" => Some(Self::Invariant),
            "approval" => Some(Self::Approval),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Source record was retrieved as this evidence.
    Retrieved,
    /// Evidence or decision supports another decision.
    Supports,
    /// Decision or evidence is a prerequisite of an action.
    Requires,
    /// Decision or evidence authorizes an approval or action.
    Authorizes,
    /// A change of the source *may* invalidate the target (never auto-false).
    Invalidates,
    /// Derived-from (decision ← evidence).
    DerivedFrom,
}

impl EdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Retrieved => "retrieved",
            EdgeKind::Supports => "supports",
            EdgeKind::Requires => "requires",
            EdgeKind::Authorizes => "authorizes",
            EdgeKind::Invalidates => "invalidates",
            EdgeKind::DerivedFrom => "derived_from",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "retrieved" => Some(Self::Retrieved),
            "supports" => Some(Self::Supports),
            "requires" => Some(Self::Requires),
            "authorizes" => Some(Self::Authorizes),
            "invalidates" => Some(Self::Invalidates),
            "derived_from" => Some(Self::DerivedFrom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecisionStatus {
    Pending,
    Valid,
    Invalid,
    Inconclusive,
    PendingRevalidation,
}

impl DecisionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionStatus::Pending => "PENDING",
            DecisionStatus::Valid => "VALID",
            DecisionStatus::Invalid => "INVALID",
            DecisionStatus::Inconclusive => "INCONCLUSIVE",
            DecisionStatus::PendingRevalidation => "PENDING_REVALIDATION",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "PENDING" => Some(Self::Pending),
            "VALID" => Some(Self::Valid),
            "INVALID" => Some(Self::Invalid),
            "INCONCLUSIVE" => Some(Self::Inconclusive),
            "PENDING_REVALIDATION" => Some(Self::PendingRevalidation),
            _ => None,
        }
    }
}

/// Independent of dispatch state. Records what recovery/reconciliation still needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryDisposition {
    None,
    /// Dispatch was never confirmed (DISPATCH_UNCERTAIN).
    UncertainDispatch,
    /// Write was accepted; settlement never confirmed. Do not treat dispatch as uncertain.
    ManualReconciliationRequired,
    /// Evidence changed; dependents must be rechecked before a sensitive write.
    RevalidationRequired,
}

impl RecoveryDisposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecoveryDisposition::None => "NONE",
            RecoveryDisposition::UncertainDispatch => "UNCERTAIN_DISPATCH",
            RecoveryDisposition::ManualReconciliationRequired => "MANUAL_RECONCILIATION_REQUIRED",
            RecoveryDisposition::RevalidationRequired => "REVALIDATION_REQUIRED",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "NONE" | "" => Some(Self::None),
            "UNCERTAIN_DISPATCH" => Some(Self::UncertainDispatch),
            "MANUAL_RECONCILIATION_REQUIRED" => Some(Self::ManualReconciliationRequired),
            "REVALIDATION_REQUIRED" => Some(Self::RevalidationRequired),
            _ => None,
        }
    }
}

/// Versioned observation of an external resource. Full source content is optional
/// and should be omitted when a fingerprint or version is enough.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct EvidenceRecord {
    pub id: Uuid,
    pub source_system: String,
    pub resource_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    pub retrieved_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    pub content_fingerprint: String,
    pub trust: TrustClassification,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_scope: Option<String>,
    #[serde(default)]
    pub dependent_decision_ids: Vec<String>,
    #[serde(default)]
    pub dependent_action_ids: Vec<String>,
    /// Explicitly registered; never inferred from model hidden state.
    #[serde(default)]
    pub inferred: bool,
}

/// Worker/tool hint attached as `_evidence` on a tool result or `_resources` on a snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema, Default)]
pub struct EvidenceHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub source_system: String,
    pub resource_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<TrustClassification>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    /// Logical coverage keys this fact satisfies, e.g. `policy.coverage`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema, Default)]
pub struct DecisionHint {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    /// Explicit write this decision is a prerequisite of, e.g. `act:authorize_payout`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ResourceChange {
    pub resource_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    /// `None` means the resource was deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    #[serde(default)]
    pub inaccessible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_and_status_round_trip() {
        for t in [
            TrustClassification::Authoritative,
            TrustClassification::Derived,
            TrustClassification::Untrusted,
            TrustClassification::Inaccessible,
        ] {
            assert_eq!(TrustClassification::parse(t.as_str()), Some(t));
        }
        for s in [
            DecisionStatus::Pending,
            DecisionStatus::Valid,
            DecisionStatus::Invalid,
            DecisionStatus::Inconclusive,
            DecisionStatus::PendingRevalidation,
        ] {
            assert_eq!(DecisionStatus::parse(s.as_str()), Some(s));
        }
        for d in [
            RecoveryDisposition::None,
            RecoveryDisposition::UncertainDispatch,
            RecoveryDisposition::ManualReconciliationRequired,
            RecoveryDisposition::RevalidationRequired,
        ] {
            assert_eq!(RecoveryDisposition::parse(d.as_str()), Some(d));
        }
    }
}
