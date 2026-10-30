//! Deterministic selective revalidation planner.
//!
//! Does not call an LLM. Explicit rules first. Graph checks never replace row locks
//! or transaction-level authorization.

use axiom_protocol::{DecisionStatus, ResourceChange};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::impact::{compute_impact, EvidenceGraph, ImpactClassification, ImpactReport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RevalidationAction {
    None,
    RecheckPredicates,
    InvalidateApproval,
    BlockSensitiveWrite,
    MarkInconclusive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RevalidationPlan {
    pub impact: ImpactReport,
    pub decisions_to_recheck: Vec<String>,
    pub predicates_to_recheck: Vec<String>,
    pub approvals_to_invalidate: Vec<String>,
    pub actions_to_block: Vec<String>,
    pub retrieve_fresh_evidence: bool,
    pub action: RevalidationAction,
}

/// Build a plan from an impact report. Affected decisions are *not* marked incorrect here.
pub fn plan_revalidation(impact: ImpactReport) -> RevalidationPlan {
    let empty = impact.affected_decisions.is_empty()
        && impact.pending_actions.is_empty()
        && impact.approvals.is_empty()
        && impact.predicates.is_empty();
    if empty {
        return RevalidationPlan {
            impact,
            decisions_to_recheck: vec![],
            predicates_to_recheck: vec![],
            approvals_to_invalidate: vec![],
            actions_to_block: vec![],
            retrieve_fresh_evidence: false,
            action: RevalidationAction::None,
        };
    }
    let action = if impact.classification == ImpactClassification::Inconclusive {
        RevalidationAction::MarkInconclusive
    } else if !impact.pending_actions.is_empty() {
        RevalidationAction::BlockSensitiveWrite
    } else if !impact.approvals.is_empty() {
        RevalidationAction::InvalidateApproval
    } else {
        RevalidationAction::RecheckPredicates
    };
    RevalidationPlan {
        decisions_to_recheck: impact.affected_decisions.clone(),
        predicates_to_recheck: impact.predicates.clone().into_iter().chain(impact.invariants.clone()).collect(),
        approvals_to_invalidate: impact.approvals.clone(),
        actions_to_block: impact.pending_actions.clone(),
        retrieve_fresh_evidence: true,
        action,
        impact,
    }
}

pub fn impact_and_plan(graph: &EvidenceGraph, changes: &[ResourceChange]) -> RevalidationPlan {
    plan_revalidation(compute_impact(graph, changes))
}

/// Explicit coverage rule: current policy must still cover the recommended amount.
pub fn coverage_still_holds(snapshot: &Value, policy_id: &str, recommended_cents: i64) -> Option<bool> {
    let key = policy_id.strip_prefix("policy:").unwrap_or(policy_id);
    let policies = snapshot.get("policies")?.as_object()?;
    let p = policies.get(key)?;
    let covered = p.get("coverage").and_then(|v| v.as_bool()).unwrap_or(false);
    let reserve = p.get("reserve_cents").and_then(|v| v.as_i64()).unwrap_or(0);
    Some(covered && recommended_cents <= reserve)
}

/// Explicit eligibility rule: order still eligible and return status still received/started.
pub fn eligibility_still_holds(snapshot: &Value, order_id: &str) -> Option<bool> {
    let key = order_id.strip_prefix("order:").unwrap_or(order_id);
    let orders = snapshot.get("orders")?.as_object()?;
    let o = orders.get(key)?;
    let eligible = o.get("eligible").and_then(|v| v.as_bool()).unwrap_or(false);
    let status = o.get("return_status").and_then(|v| v.as_str()).unwrap_or("");
    let ok_status = status.is_empty() || status == "received" || status == "started" || status == "none";
    Some(eligible && ok_status && o.get("status").and_then(|v| v.as_str()) != Some("refunded"))
}

pub fn apply_rule(kind: &str, snapshot: &Value, resource_id: &str, payload: &Value) -> Option<bool> {
    match kind {
        "coverage" => {
            let amount = payload.get("amount_cents").and_then(|v| v.as_i64()).unwrap_or(0);
            coverage_still_holds(snapshot, resource_id, amount)
        }
        "eligibility" => eligibility_still_holds(snapshot, resource_id),
        _ => None,
    }
}

pub fn decision_after_rule(current_version: Option<&str>, passed: Option<bool>) -> DecisionStatus {
    crate::impact::classify_revalidation(DecisionStatus::PendingRevalidation, current_version, passed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::impact::{EvidenceGraph, GraphEdge, GraphNode};
    use axiom_protocol::{EdgeKind, NodeKind};

    #[test]
    fn stale_approval_blocked_when_policy_changes() {
        let mut g = EvidenceGraph::new();
        g.insert_node(GraphNode {
            id: "ev".into(),
            kind: NodeKind::Evidence,
            resource_id: Some("policy:POL-1".into()),
            version: Some("4".into()),
            label: None,
        });
        g.insert_node(GraphNode { id: "dec".into(), kind: NodeKind::Decision, resource_id: None, version: None, label: None });
        g.insert_node(GraphNode { id: "appr".into(), kind: NodeKind::Approval, resource_id: None, version: None, label: None });
        g.insert_node(GraphNode { id: "act".into(), kind: NodeKind::Action, resource_id: None, version: None, label: None });
        g.insert_edge(GraphEdge { from: "ev".into(), to: "dec".into(), kind: EdgeKind::Supports });
        g.insert_edge(GraphEdge { from: "dec".into(), to: "appr".into(), kind: EdgeKind::Authorizes });
        g.insert_edge(GraphEdge { from: "dec".into(), to: "act".into(), kind: EdgeKind::Requires });
        let plan = impact_and_plan(
            &g,
            &[ResourceChange {
                resource_id: "policy:POL-1".into(),
                old_version: Some("4".into()),
                new_version: Some("5".into()),
                inaccessible: false,
            }],
        );
        assert_eq!(plan.action, RevalidationAction::BlockSensitiveWrite);
        assert!(plan.approvals_to_invalidate.contains(&"appr".into()));
        assert!(plan.actions_to_block.contains(&"act".into()));
    }

    #[test]
    fn coverage_rule_fails_when_reserve_drops() {
        let snap = serde_json::json!({"policies": {"POL-1": {"coverage": true, "reserve_cents": 5000}}});
        assert_eq!(coverage_still_holds(&snap, "policy:POL-1", 15000), Some(false));
        assert_eq!(coverage_still_holds(&snap, "policy:POL-1", 4000), Some(true));
    }

    #[test]
    fn eligibility_fails_on_rejected_return() {
        let snap = serde_json::json!({"orders": {"ORD-2048": {"eligible": false, "return_status": "rejected", "status": "paid"}}});
        assert_eq!(eligibility_still_holds(&snap, "order:ORD-2048"), Some(false));
    }

    #[test]
    fn unknown_rule_is_inconclusive() {
        assert_eq!(apply_rule("mystery", &serde_json::json!({}), "x", &serde_json::json!({})), None);
        assert_eq!(decision_after_rule(Some("1"), None), DecisionStatus::Inconclusive);
    }
}
