//! Risk-aware action gates. Deterministic. The model cannot override them.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use goshikq_protocol::{
    ApprovalStatus, CoverageStatus, EffectCategory, EvidenceCoverageContract, GateDecisionKind,
    MissingEvidenceBehavior, RevalidationPolicy, RiskLevel, ToolSpec,
};

use crate::baselines::StrategyOutput;
use crate::coverage::CoverageReport;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GateDecision {
    pub kind: GateDecisionKind,
    pub reasons: Vec<String>,
    pub policy: RevalidationPolicy,
    pub coverage_status: Option<CoverageStatus>,
    pub requires_human_review: bool,
}

impl GateDecision {
    pub fn allow(policy: RevalidationPolicy) -> Self {
        GateDecision {
            kind: GateDecisionKind::Allow,
            reasons: vec![],
            policy,
            coverage_status: None,
            requires_human_review: false,
        }
    }

    pub fn is_block(&self) -> bool {
        matches!(
            self.kind,
            GateDecisionKind::Block | GateDecisionKind::RequireApproval
        )
    }
}

fn is_high_risk(spec: &ToolSpec, contract: Option<&EvidenceCoverageContract>) -> bool {
    let risk = contract.map(|c| c.risk).unwrap_or(spec.risk);
    matches!(risk, RiskLevel::High | RiskLevel::Critical)
        || matches!(
            spec.effect,
            EffectCategory::NonIdempotentWrite | EffectCategory::Irreversible
        )
}

fn is_read_only(spec: &ToolSpec) -> bool {
    spec.effect == EffectCategory::ReadOnly
}

/// Combine coverage, revalidation output, approval state, and risk.
///
/// Unknown dependencies remain unknown: a complete coverage report does not
/// authorize a goal, and an empty contract does not invent requirements.
pub fn evaluate_action_gate(
    spec: &ToolSpec,
    coverage: Option<&CoverageReport>,
    predicted: &StrategyOutput,
    approval: Option<ApprovalStatus>,
) -> GateDecision {
    let policy = spec.revalidation_policy;
    let contract = spec.evidence_coverage.as_ref();
    let high = is_high_risk(spec, contract);
    let mut reasons = Vec::new();
    let mut kind = GateDecisionKind::Allow;
    let mut review = false;

    if let Some(rep) = coverage {
        match rep.status {
            CoverageStatus::CompleteForDeclaredContract => {}
            CoverageStatus::Incomplete => {
                reasons.extend(
                    rep.reasons
                        .iter()
                        .map(|r| format!("{}: {}", r.code, r.detail)),
                );
                apply_missing(
                    spec,
                    contract,
                    high,
                    &mut kind,
                    &mut reasons,
                    "missing required evidence",
                );
            }
            CoverageStatus::Stale => {
                reasons.extend(
                    rep.reasons
                        .iter()
                        .map(|r| format!("{}: {}", r.code, r.detail)),
                );
                if high || !is_read_only(spec) {
                    kind = GateDecisionKind::Block;
                    reasons.push("stale required evidence cannot authorize this action".into());
                } else {
                    bump_warn(&mut kind);
                    reasons.push("stale evidence on a read-only action".into());
                }
            }
            CoverageStatus::Conflicted => {
                kind = GateDecisionKind::Block;
                reasons.push("conflicted required evidence".into());
            }
            CoverageStatus::Inconclusive => {
                reasons.push("required verification is inconclusive".into());
                if high || spec.revalidation_policy == RevalidationPolicy::StrictContract {
                    kind = GateDecisionKind::Block;
                } else if is_read_only(spec) {
                    bump_warn(&mut kind);
                } else {
                    kind = GateDecisionKind::Block;
                }
            }
        }
    } else if policy == RevalidationPolicy::StrictContract && high {
        kind = GateDecisionKind::Block;
        reasons.push("STRICT_CONTRACT high-risk action has no coverage evaluation".into());
    }

    if !predicted.actions_to_block.is_empty()
        && matches!(
            policy,
            RevalidationPolicy::Selective
                | RevalidationPolicy::FullDeclared
                | RevalidationPolicy::SelectivePlusPrewrite
                | RevalidationPolicy::StrictContract
        )
        && spec.name != ""
    {
        // Caller is expected to only pass predictions for *this* action. If this
        // action id/name is listed, block.
        let act_id = format!("act:{}", spec.name);
        let blocked_here = predicted.actions_to_block.iter().any(|id| {
            id == &spec.name || id == &act_id || id.ends_with(&format!(":{}", spec.name))
        });
        if blocked_here {
            kind = GateDecisionKind::Block;
            reasons.push("revalidation policy blocked this write".into());
        }
    }

    if spec.requires_approval {
        match approval {
            Some(ApprovalStatus::Approved) => {}
            Some(ApprovalStatus::Invalidated) => {
                kind = GateDecisionKind::Block;
                reasons.push(
                    "approval was invalidated; stale approval cannot authorize a write".into(),
                );
            }
            Some(ApprovalStatus::Denied) | Some(ApprovalStatus::Expired) => {
                kind = GateDecisionKind::Block;
                reasons.push(format!("approval is {:?}", approval));
            }
            Some(ApprovalStatus::Pending) | None => {
                if high {
                    kind = GateDecisionKind::RequireApproval;
                    review = true;
                    reasons.push("explicit approval required".into());
                }
            }
        }
    }

    if !predicted.approvals_to_invalidate.is_empty() && approval == Some(ApprovalStatus::Approved) {
        kind = GateDecisionKind::Block;
        reasons.push("approval must be re-issued against current evidence".into());
    }

    GateDecision {
        kind,
        reasons,
        policy,
        coverage_status: coverage.map(|c| c.status),
        requires_human_review: review || kind == GateDecisionKind::RequireApproval,
    }
}

fn apply_missing(
    spec: &ToolSpec,
    contract: Option<&EvidenceCoverageContract>,
    high: bool,
    kind: &mut GateDecisionKind,
    reasons: &mut Vec<String>,
    msg: &str,
) {
    let behavior = contract
        .map(|c| c.on_missing)
        .unwrap_or(MissingEvidenceBehavior::Block);
    if high {
        *kind = GateDecisionKind::Block;
        reasons.push(format!("{msg} (high-risk actions cannot proceed)"));
        return;
    }
    if is_read_only(spec) {
        match behavior {
            MissingEvidenceBehavior::Allow => {}
            MissingEvidenceBehavior::Warn => {
                bump_warn(kind);
                reasons.push(msg.into());
            }
            MissingEvidenceBehavior::Block => {
                *kind = GateDecisionKind::Block;
                reasons.push(msg.into());
            }
        }
        return;
    }
    match behavior {
        MissingEvidenceBehavior::Allow if !high => bump_warn(kind),
        _ => {
            *kind = GateDecisionKind::Block;
            reasons.push(msg.into());
        }
    }
}

fn bump_warn(kind: &mut GateDecisionKind) {
    if *kind == GateDecisionKind::Allow {
        *kind = GateDecisionKind::Warn;
    }
}

/// Map a tool name to whether `predicted` listed it. Used by the executor with node ids.
pub fn prediction_blocks_action(predicted: &StrategyOutput, action_ids: &[&str]) -> bool {
    predicted
        .actions_to_block
        .iter()
        .any(|id| action_ids.iter().any(|a| a == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::{evaluate_coverage, CoverageReport};
    use chrono::Utc;
    use goshikq_protocol::{
        EvidenceFact, RequiredEvidence, ResourceChange, RetryPolicy, TrustClassification,
    };
    use serde_json::json;

    fn spec(risk: RiskLevel, effect: EffectCategory, policy: RevalidationPolicy) -> ToolSpec {
        ToolSpec {
            name: "authorize_payout".into(),
            description: String::new(),
            input_schema: json!({"type": "object"}),
            output_schema: None,
            effect,
            risk,
            timeout_ms: 1000,
            retry_policy: RetryPolicy::default(),
            idempotent: false,
            requires_approval: true,
            required_permissions: vec!["claims:payout".into()],
            preconditions: vec![],
            has_verifier: false,
            revalidation_policy: policy,
            evidence_coverage: Some(EvidenceCoverageContract {
                required: vec![RequiredEvidence::from_key("policy:POL-1")],
                required_invariants: vec!["coverage".into()],
                required_authorization: vec![],
                on_missing: MissingEvidenceBehavior::Block,
                risk,
            }),
        }
    }

    #[test]
    fn high_risk_missing_evidence_blocks() {
        let s = spec(
            RiskLevel::Critical,
            EffectCategory::NonIdempotentWrite,
            RevalidationPolicy::StrictContract,
        );
        let cov = evaluate_coverage(
            s.evidence_coverage.as_ref().unwrap(),
            &[],
            &[],
            &[],
            Utc::now(),
        );
        let g = evaluate_action_gate(
            &s,
            Some(&cov),
            &StrategyOutput::default(),
            Some(ApprovalStatus::Approved),
        );
        assert_eq!(g.kind, GateDecisionKind::Block);
    }

    #[test]
    fn stale_blocks_even_with_approval() {
        let s = spec(
            RiskLevel::Critical,
            EffectCategory::NonIdempotentWrite,
            RevalidationPolicy::StrictContract,
        );
        let fact = EvidenceFact {
            id: "e1".into(),
            resource_id: "policy:POL-1".into(),
            keys: vec![],
            version: Some("4".into()),
            trust: TrustClassification::Authoritative,
            retrieved_at: Utc::now(),
            current_version: Some("5".into()),
            inaccessible: false,
            fingerprint: Some("fp".into()),
            source_system: None,
        };
        let cov = evaluate_coverage(
            s.evidence_coverage.as_ref().unwrap(),
            &[fact],
            &[goshikq_protocol::DecisionFact {
                id: "d".into(),
                kind: "coverage".into(),
                status: "VALID".into(),
            }],
            &[ResourceChange {
                resource_id: "policy:POL-1".into(),
                old_version: Some("4".into()),
                new_version: Some("5".into()),
                inaccessible: false,
            }],
            Utc::now(),
        );
        let g = evaluate_action_gate(
            &s,
            Some(&cov),
            &StrategyOutput::default(),
            Some(ApprovalStatus::Approved),
        );
        assert_eq!(g.kind, GateDecisionKind::Block);
        assert!(g.reasons.iter().any(|r| r.contains("stale")));
    }

    #[test]
    fn read_only_can_warn() {
        let mut s = spec(
            RiskLevel::Low,
            EffectCategory::ReadOnly,
            RevalidationPolicy::Selective,
        );
        s.requires_approval = false;
        if let Some(c) = s.evidence_coverage.as_mut() {
            c.on_missing = MissingEvidenceBehavior::Warn;
            c.risk = RiskLevel::Low;
        }
        let cov = evaluate_coverage(
            s.evidence_coverage.as_ref().unwrap(),
            &[],
            &[],
            &[],
            Utc::now(),
        );
        let g = evaluate_action_gate(&s, Some(&cov), &StrategyOutput::default(), None);
        assert_eq!(g.kind, GateDecisionKind::Warn);
    }

    #[test]
    fn invalidated_approval_cannot_authorize() {
        let s = spec(
            RiskLevel::High,
            EffectCategory::NonIdempotentWrite,
            RevalidationPolicy::Selective,
        );
        let g = evaluate_action_gate(
            &s,
            Some(&CoverageReport::complete()),
            &StrategyOutput::default(),
            Some(ApprovalStatus::Invalidated),
        );
        assert_eq!(g.kind, GateDecisionKind::Block);
    }

    #[test]
    fn complete_coverage_is_not_goal_verified() {
        let s = spec(
            RiskLevel::High,
            EffectCategory::NonIdempotentWrite,
            RevalidationPolicy::StrictContract,
        );
        let g = evaluate_action_gate(
            &s,
            Some(&CoverageReport::complete()),
            &StrategyOutput::default(),
            Some(ApprovalStatus::Approved),
        );
        assert_eq!(g.kind, GateDecisionKind::Allow);
        assert_ne!(g.coverage_status.unwrap().as_str(), "VERIFIED");
    }
}
