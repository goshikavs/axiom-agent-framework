//! Four independent revalidation strategies for evidence-change experiments.
//!
//! These are intentionally different algorithms, not aliases of [`compute_impact`]:
//!
//! - **A `None`:** never revalidate. Pending writes proceed.
//! - **B `FullDeclared`:** if *any* registered resource changed, recheck *every*
//!   declared decision, action, and approval in the graph.
//! - **C `ConventionalPrewrite`:** immediately before a sensitive action, compare
//!   versions of resources the *action node itself* declared (`resource_id`).
//!   No graph walk. Decisions are not marked.
//! - **D `Selective`:** Axiom dependency-aware impact (`compute_impact`).
//!
//! None of these infers unregistered (hidden) dependencies.

use std::collections::BTreeSet;

use axiom_protocol::{EvidenceCoverageContract, NodeKind, ResourceChange};
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::coverage::evaluate_coverage;
use crate::impact::{compute_impact, EvidenceGraph};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RevalidationStrategy {
    /// A. No evidence revalidation.
    None,
    /// B. Full revalidation of all declared nodes after any hitting change.
    FullDeclared,
    /// C. Conventional pre-write version check on the action's declared resource.
    ConventionalPrewrite,
    /// D. Selective dependency-aware revalidation (Axiom impact engine).
    Selective,
    /// E. Selective walk plus conventional pre-write checks.
    SelectivePlusPrewrite,
    /// F. Coverage contract + selective walk + pre-write.
    StrictContract,
}

impl RevalidationStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            RevalidationStrategy::None => "none",
            RevalidationStrategy::FullDeclared => "full_declared",
            RevalidationStrategy::ConventionalPrewrite => "conventional_prewrite",
            RevalidationStrategy::Selective => "selective",
            RevalidationStrategy::SelectivePlusPrewrite => "selective_plus_prewrite",
            RevalidationStrategy::StrictContract => "strict_contract",
        }
    }
}

/// Optional context for E/F. Ignored by A–D.
#[derive(Debug, Clone, Default)]
pub struct StrategyContext {
    pub contracts: std::collections::BTreeMap<String, EvidenceCoverageContract>,
    pub facts: Vec<axiom_protocol::EvidenceFact>,
    /// Decision rows used by STRICT_CONTRACT coverage (invariants / verification_method).
    pub decisions: Vec<axiom_protocol::DecisionFact>,
    /// When unset, F uses the seeded bench clock (2026-01-01) so A–F stay reproducible.
    pub now: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
pub struct StrategyOutput {
    pub decisions_to_recheck: Vec<String>,
    pub actions_to_block: Vec<String>,
    pub approvals_to_invalidate: Vec<String>,
}

impl StrategyOutput {
    pub fn sorted(mut self) -> Self {
        self.decisions_to_recheck.sort();
        self.actions_to_block.sort();
        self.approvals_to_invalidate.sort();
        self
    }
}

fn change_hits(change: &ResourceChange, recorded_version: Option<&str>) -> bool {
    if change.inaccessible {
        return true;
    }
    if change.new_version.is_none() {
        return true;
    }
    match (recorded_version, change.new_version.as_deref()) {
        (Some(old), Some(new)) => old != new,
        (None, Some(_)) => true,
        _ => change.old_version != change.new_version,
    }
}

fn hitting_resources(graph: &EvidenceGraph, changes: &[ResourceChange]) -> BTreeSet<String> {
    let mut hits = BTreeSet::new();
    for c in changes {
        let nodes = graph.nodes_for_resource(&c.resource_id);
        if nodes.is_empty() {
            continue;
        }
        if nodes.iter().any(|n| change_hits(c, n.version.as_deref())) || c.inaccessible {
            hits.insert(c.resource_id.clone());
        }
    }
    hits
}

/// A. Ignore all evidence changes.
pub fn strategy_none(_graph: &EvidenceGraph, _changes: &[ResourceChange]) -> StrategyOutput {
    StrategyOutput::default()
}

/// B. Any hitting change on a registered resource → revalidate the entire declared graph.
pub fn strategy_full_declared(graph: &EvidenceGraph, changes: &[ResourceChange]) -> StrategyOutput {
    if hitting_resources(graph, changes).is_empty() {
        return StrategyOutput::default();
    }
    StrategyOutput {
        decisions_to_recheck: graph.ids_by_kind(NodeKind::Decision),
        actions_to_block: graph.ids_by_kind(NodeKind::Action),
        approvals_to_invalidate: graph.ids_by_kind(NodeKind::Approval),
    }
    .sorted()
}

/// C. Block an action only when the action node declared `resource_id` and that resource changed.
/// Decisions and approvals are not selected (no dependency walk).
pub fn strategy_conventional_prewrite(graph: &EvidenceGraph, changes: &[ResourceChange]) -> StrategyOutput {
    let hits = hitting_resources(graph, changes);
    let mut actions = BTreeSet::new();
    for node in graph.nodes() {
        if node.kind != NodeKind::Action {
            continue;
        }
        if let Some(rid) = &node.resource_id {
            if hits.contains(rid) {
                actions.insert(node.id.clone());
            }
        }
    }
    StrategyOutput {
        decisions_to_recheck: vec![],
        actions_to_block: actions.into_iter().collect(),
        approvals_to_invalidate: vec![],
    }
}

/// D. Selective graph walk (existing Axiom engine).
pub fn strategy_selective(graph: &EvidenceGraph, changes: &[ResourceChange]) -> StrategyOutput {
    let report = compute_impact(graph, changes);
    StrategyOutput {
        decisions_to_recheck: report.affected_decisions,
        actions_to_block: report.pending_actions,
        approvals_to_invalidate: report.approvals,
    }
    .sorted()
}

    fn union_outputs(a: StrategyOutput, b: StrategyOutput) -> StrategyOutput {
    let d: BTreeSet<_> = a.decisions_to_recheck.into_iter().chain(b.decisions_to_recheck).collect();
    let act: BTreeSet<_> = a.actions_to_block.into_iter().chain(b.actions_to_block).collect();
    let ap: BTreeSet<_> = a.approvals_to_invalidate.into_iter().chain(b.approvals_to_invalidate).collect();
    StrategyOutput {
        decisions_to_recheck: d.into_iter().collect(),
        actions_to_block: act.into_iter().collect(),
        approvals_to_invalidate: ap.into_iter().collect(),
    }
}

/// E. Selective ∪ conventional pre-write.
pub fn strategy_selective_plus_prewrite(graph: &EvidenceGraph, changes: &[ResourceChange]) -> StrategyOutput {
    union_outputs(strategy_selective(graph, changes), strategy_conventional_prewrite(graph, changes))
}

/// F. E plus contract coverage: stale/missing declared requirements block the action.
pub fn strategy_strict_contract(graph: &EvidenceGraph, changes: &[ResourceChange], ctx: &StrategyContext) -> StrategyOutput {
    let mut out = strategy_selective_plus_prewrite(graph, changes);
    let now = ctx.now.unwrap_or_else(|| Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    let mut extra = BTreeSet::new();
    for node in graph.nodes() {
        if node.kind != NodeKind::Action {
            continue;
        }
        let Some(contract) = ctx.contracts.get(&node.id) else {
            continue;
        };
        if contract.is_empty() {
            continue;
        }
        let report = evaluate_coverage(contract, &ctx.facts, &ctx.decisions, changes, now);
        if !report.is_complete() {
            extra.insert(node.id.clone());
        }
    }
    out.actions_to_block.extend(extra);
    out.sorted()
}

pub fn run_strategy(strategy: RevalidationStrategy, graph: &EvidenceGraph, changes: &[ResourceChange]) -> StrategyOutput {
    run_strategy_with_context(strategy, graph, changes, &StrategyContext::default())
}

pub fn run_strategy_with_context(
    strategy: RevalidationStrategy,
    graph: &EvidenceGraph,
    changes: &[ResourceChange],
    ctx: &StrategyContext,
) -> StrategyOutput {
    match strategy {
        RevalidationStrategy::None => strategy_none(graph, changes),
        RevalidationStrategy::FullDeclared => strategy_full_declared(graph, changes),
        RevalidationStrategy::ConventionalPrewrite => strategy_conventional_prewrite(graph, changes),
        RevalidationStrategy::Selective => strategy_selective(graph, changes),
        RevalidationStrategy::SelectivePlusPrewrite => strategy_selective_plus_prewrite(graph, changes),
        RevalidationStrategy::StrictContract => strategy_strict_contract(graph, changes, ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::impact::{GraphEdge, GraphNode};
    use axiom_protocol::EdgeKind;

    fn fixture() -> EvidenceGraph {
        let mut g = EvidenceGraph::new();
        g.insert_node(GraphNode {
            id: "ev-pol".into(),
            kind: NodeKind::Evidence,
            resource_id: Some("policy:POL-1".into()),
            version: Some("4".into()),
            label: None,
        });
        g.insert_node(GraphNode {
            id: "dec-cov".into(),
            kind: NodeKind::Decision,
            resource_id: None,
            version: None,
            label: None,
        });
        g.insert_node(GraphNode {
            id: "dec-other".into(),
            kind: NodeKind::Decision,
            resource_id: None,
            version: None,
            label: None,
        });
        g.insert_node(GraphNode {
            id: "ev-ord".into(),
            kind: NodeKind::Evidence,
            resource_id: Some("order:ORD-1".into()),
            version: Some("1".into()),
            label: None,
        });
        g.insert_node(GraphNode {
            id: "act-pay".into(),
            kind: NodeKind::Action,
            resource_id: None, // tool did not declare the policy resource
            version: None,
            label: None,
        });
        g.insert_node(GraphNode {
            id: "act-declared".into(),
            kind: NodeKind::Action,
            resource_id: Some("policy:POL-1".into()),
            version: Some("4".into()),
            label: None,
        });
        g.insert_node(GraphNode {
            id: "appr".into(),
            kind: NodeKind::Approval,
            resource_id: None,
            version: None,
            label: None,
        });
        g.insert_edge(GraphEdge { from: "ev-pol".into(), to: "dec-cov".into(), kind: EdgeKind::Supports });
        g.insert_edge(GraphEdge { from: "dec-cov".into(), to: "act-pay".into(), kind: EdgeKind::Requires });
        g.insert_edge(GraphEdge { from: "dec-cov".into(), to: "appr".into(), kind: EdgeKind::Authorizes });
        g.insert_edge(GraphEdge { from: "ev-ord".into(), to: "dec-other".into(), kind: EdgeKind::Supports });
        g
    }

    fn pol_change() -> ResourceChange {
        ResourceChange {
            resource_id: "policy:POL-1".into(),
            old_version: Some("4".into()),
            new_version: Some("5".into()),
            inaccessible: false,
        }
    }

    #[test]
    fn four_strategies_are_not_aliases() {
        let g = fixture();
        let c = [pol_change()];
        let a = run_strategy(RevalidationStrategy::None, &g, &c);
        let b = run_strategy(RevalidationStrategy::FullDeclared, &g, &c);
        let conv = run_strategy(RevalidationStrategy::ConventionalPrewrite, &g, &c);
        let d = run_strategy(RevalidationStrategy::Selective, &g, &c);
        assert!(a.decisions_to_recheck.is_empty() && a.actions_to_block.is_empty());
        assert!(b.decisions_to_recheck.contains(&"dec-cov".into()));
        assert!(b.decisions_to_recheck.contains(&"dec-other".into()), "full declared rechecks unrelated decisions");
        assert!(!conv.decisions_to_recheck.contains(&"dec-cov".into()), "conventional does not walk to decisions");
        assert!(conv.actions_to_block.contains(&"act-declared".into()));
        assert!(!conv.actions_to_block.contains(&"act-pay".into()), "undeclared action is not version-checked");
        assert!(d.decisions_to_recheck.contains(&"dec-cov".into()));
        assert!(!d.decisions_to_recheck.contains(&"dec-other".into()), "selective leaves unrelated decisions");
        assert!(d.actions_to_block.contains(&"act-pay".into()));
        assert_ne!(b.decisions_to_recheck, d.decisions_to_recheck);
        assert_ne!(conv.actions_to_block, d.actions_to_block);
    }

    #[test]
    fn none_never_blocks() {
        let g = fixture();
        let out = strategy_none(&g, &[pol_change()]);
        assert!(out.actions_to_block.is_empty());
    }

    #[test]
    fn conventional_ignores_unrelated_order_change() {
        let g = fixture();
        let c = [ResourceChange {
            resource_id: "order:ORD-1".into(),
            old_version: Some("1".into()),
            new_version: Some("2".into()),
            inaccessible: false,
        }];
        let conv = strategy_conventional_prewrite(&g, &c);
        assert!(conv.actions_to_block.is_empty());
        let d = strategy_selective(&g, &c);
        assert!(d.decisions_to_recheck.contains(&"dec-other".into()));
        assert!(!d.decisions_to_recheck.contains(&"dec-cov".into()));
    }

    #[test]
    fn plus_prewrite_and_strict_are_distinct_from_selective() {
        let c = [pol_change()];
        let mut g = fixture();
        // Version already equals the new version: selective does not seed this action,
        // conventional still blocks because the resource hit on evidence.
        g.insert_node(GraphNode {
            id: "act-prewrite".into(),
            kind: NodeKind::Action,
            resource_id: Some("policy:POL-1".into()),
            version: Some("5".into()),
            label: None,
        });
        g.insert_node(GraphNode {
            id: "act-orphan".into(),
            kind: NodeKind::Action,
            resource_id: None,
            version: None,
            label: None,
        });
        let d = strategy_selective(&g, &c);
        let e = strategy_selective_plus_prewrite(&g, &c);
        assert!(e.actions_to_block.contains(&"act-prewrite".into()));
        assert!(!d.actions_to_block.contains(&"act-prewrite".into()));
        assert!(e.actions_to_block.contains(&"act-pay".into()));
        assert_ne!(d.actions_to_block, e.actions_to_block);

        let mut ctx = StrategyContext::default();
        ctx.contracts.insert(
            "act-orphan".into(),
            axiom_protocol::EvidenceCoverageContract {
                required: vec![axiom_protocol::RequiredEvidence::from_key("policy:POL-1")],
                ..Default::default()
            },
        );
        ctx.facts.push(axiom_protocol::EvidenceFact {
            id: "ev-pol".into(),
            resource_id: "policy:POL-1".into(),
            keys: vec![],
            version: Some("4".into()),
            trust: axiom_protocol::TrustClassification::Authoritative,
            retrieved_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            current_version: Some("5".into()),
            inaccessible: false,
            fingerprint: Some("fp4".into()),
            source_system: None,
        });
        let f = strategy_strict_contract(&g, &c, &ctx);
        assert!(f.actions_to_block.contains(&"act-orphan".into()), "STRICT blocks via declared contract, not graph");
        assert!(!e.actions_to_block.contains(&"act-orphan".into()));
        assert_ne!(e, f);
    }

    #[test]
    fn strict_contract_does_not_false_block_when_invariants_are_recorded() {
        let g = fixture();
        let mut ctx = StrategyContext::default();
        ctx.contracts.insert(
            "act-pay".into(),
            axiom_protocol::EvidenceCoverageContract {
                required: vec![axiom_protocol::RequiredEvidence::from_key("policy:POL-1")],
                required_invariants: vec!["eligibility".into()],
                ..Default::default()
            },
        );
        ctx.facts.push(axiom_protocol::EvidenceFact {
            id: "ev-pol".into(),
            resource_id: "policy:POL-1".into(),
            keys: vec![],
            version: Some("4".into()),
            trust: axiom_protocol::TrustClassification::Authoritative,
            retrieved_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            current_version: Some("4".into()),
            inaccessible: false,
            fingerprint: Some("fp4".into()),
            source_system: None,
        });
        ctx.decisions.push(axiom_protocol::DecisionFact {
            id: "dec-el".into(),
            kind: "eligibility".into(),
            status: "PENDING".into(),
        });
        let f = strategy_strict_contract(&g, &[], &ctx);
        assert!(
            !f.actions_to_block.contains(&"act-pay".into()),
            "complete declared coverage plus a live invariant decision must not extra-block"
        );
    }
}
