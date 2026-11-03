//! Evidence coverage contracts and revalidation policies.
//!
//! Coverage is completeness of **declared** requirements, never proof that every
//! fact an LLM considered is known.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{RiskLevel, TrustClassification};

/// How the executor revalidates before a sensitive write.
/// Distinct from [`crate::VerificationPolicy`] (goal verification).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema, Default,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RevalidationPolicy {
    /// Walk declared evidence edges only (`compute_impact`).
    #[default]
    Selective,
    /// Recheck every declared decision/action/approval after any registered hit.
    FullDeclared,
    /// Selective walk plus conventional action `resource_id` version check.
    SelectivePlusPrewrite,
    /// Declared evidence coverage + selective walk + authoritative prewrite + risk gates.
    StrictContract,
}

impl RevalidationPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            RevalidationPolicy::Selective => "SELECTIVE",
            RevalidationPolicy::FullDeclared => "FULL_DECLARED",
            RevalidationPolicy::SelectivePlusPrewrite => "SELECTIVE_PLUS_PREWRITE",
            RevalidationPolicy::StrictContract => "STRICT_CONTRACT",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "SELECTIVE" => Some(Self::Selective),
            "FULL_DECLARED" => Some(Self::FullDeclared),
            "SELECTIVE_PLUS_PREWRITE" => Some(Self::SelectivePlusPrewrite),
            "STRICT_CONTRACT" => Some(Self::StrictContract),
            _ => None,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum MissingEvidenceBehavior {
    #[default]
    Block,
    Warn,
    Allow,
}

impl MissingEvidenceBehavior {
    pub fn as_str(&self) -> &'static str {
        match self {
            MissingEvidenceBehavior::Block => "block",
            MissingEvidenceBehavior::Warn => "warn",
            MissingEvidenceBehavior::Allow => "allow",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "block" => Some(Self::Block),
            "warn" => Some(Self::Warn),
            "allow" => Some(Self::Allow),
            _ => None,
        }
    }
}

/// One declared requirement. Not an assertion that no other facts matter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RequiredEvidence {
    /// Logical name, e.g. `policy.coverage` or `policy:POL-1`.
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    /// Prefix match, e.g. `policy` matches `policy:POL-1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_system: Option<String>,
    #[serde(default = "default_authoritative")]
    pub min_trust: TrustClassification,
    #[serde(default = "default_true")]
    pub require_version: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_method: Option<String>,
    /// This item is authorization evidence (approval token, authority record).
    #[serde(default)]
    pub authorization: bool,
}

fn default_true() -> bool {
    true
}

fn default_authoritative() -> TrustClassification {
    TrustClassification::Authoritative
}

impl RequiredEvidence {
    pub fn from_key(key: impl Into<String>) -> Self {
        let key = key.into();
        let (resource_id, resource_family, authorization, verification_method) = parse_key(&key);
        RequiredEvidence {
            key: key.clone(),
            resource_id,
            resource_family,
            source_system: None,
            min_trust: TrustClassification::Authoritative,
            require_version: true,
            max_age_secs: None,
            verification_method,
            authorization,
        }
    }
}

fn parse_key(key: &str) -> (Option<String>, Option<String>, bool, Option<String>) {
    if key.contains(':') {
        return (Some(key.to_string()), None, false, None);
    }
    let mut parts = key.splitn(2, '.');
    let family = parts.next().map(|s| s.to_string());
    let rest = parts.next();
    let authorization = matches!(
        rest,
        Some("authority") | Some("authorization") | Some("approval")
    );
    let method = rest.map(|s| s.to_string());
    (None, family, authorization, method)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema, Default)]
pub struct EvidenceCoverageContract {
    #[serde(default)]
    pub required: Vec<RequiredEvidence>,
    #[serde(default)]
    pub required_invariants: Vec<String>,
    #[serde(default)]
    pub required_authorization: Vec<String>,
    #[serde(default)]
    pub on_missing: MissingEvidenceBehavior,
    #[serde(default)]
    pub risk: RiskLevel,
}

impl EvidenceCoverageContract {
    pub fn is_empty(&self) -> bool {
        self.required.is_empty()
            && self.required_invariants.is_empty()
            && self.required_authorization.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoverageStatus {
    CompleteForDeclaredContract,
    Incomplete,
    Stale,
    Conflicted,
    Inconclusive,
}

impl CoverageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CoverageStatus::CompleteForDeclaredContract => "COMPLETE_FOR_DECLARED_CONTRACT",
            CoverageStatus::Incomplete => "INCOMPLETE",
            CoverageStatus::Stale => "STALE",
            CoverageStatus::Conflicted => "CONFLICTED",
            CoverageStatus::Inconclusive => "INCONCLUSIVE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateDecisionKind {
    Allow,
    Warn,
    Block,
    RequireApproval,
}

impl GateDecisionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            GateDecisionKind::Allow => "ALLOW",
            GateDecisionKind::Warn => "WARN",
            GateDecisionKind::Block => "BLOCK",
            GateDecisionKind::RequireApproval => "REQUIRE_APPROVAL",
        }
    }
}

/// Observed evidence fact fed to the IO-free coverage evaluator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct EvidenceFact {
    pub id: String,
    pub resource_id: String,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub trust: TrustClassification,
    pub retrieved_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_version: Option<String>,
    #[serde(default)]
    pub inaccessible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_system: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct DecisionFact {
    pub id: String,
    pub kind: String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_key_parses_family() {
        let r = RequiredEvidence::from_key("policy.coverage");
        assert_eq!(r.resource_family.as_deref(), Some("policy"));
        assert_eq!(r.verification_method.as_deref(), Some("coverage"));
        assert!(!r.authorization);
        let a = RequiredEvidence::from_key("approval.authority");
        assert!(a.authorization);
        let id = RequiredEvidence::from_key("policy:POL-1");
        assert_eq!(id.resource_id.as_deref(), Some("policy:POL-1"));
    }
}
