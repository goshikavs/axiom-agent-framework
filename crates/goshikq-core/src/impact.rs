//! Evidence dependency graph and deterministic change-impact analysis.
//!
//! IO-free. Affected decisions are classified as *requiring verification*, never as
//! automatically incorrect. Only explicitly registered edges are traversed.

use std::collections::{BTreeMap, BTreeSet};

use goshikq_protocol::{DecisionStatus, EdgeKind, NodeKind, ResourceChange};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GraphNode {
    pub id: String,
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EvidenceGraph {
    nodes: BTreeMap<String, GraphNode>,
    /// Outgoing adjacency: from → (to, kind). BTree for deterministic walk.
    adj: BTreeMap<String, BTreeSet<(String, String)>>,
}

impl EvidenceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_node(&mut self, node: GraphNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn insert_edge(&mut self, edge: GraphEdge) {
        self.nodes
            .entry(edge.from.clone())
            .or_insert_with(|| GraphNode {
                id: edge.from.clone(),
                kind: NodeKind::Evidence,
                resource_id: None,
                version: None,
                label: None,
            });
        self.nodes
            .entry(edge.to.clone())
            .or_insert_with(|| GraphNode {
                id: edge.to.clone(),
                kind: NodeKind::Decision,
                resource_id: None,
                version: None,
                label: None,
            });
        self.adj
            .entry(edge.from.clone())
            .or_default()
            .insert((edge.to, edge.kind.as_str().to_string()));
    }

    pub fn node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &GraphNode> {
        self.nodes.values()
    }

    pub fn edges(&self) -> Vec<GraphEdge> {
        let mut out = Vec::new();
        for (from, set) in &self.adj {
            for (to, kind) in set {
                if let Some(k) = EdgeKind::parse(kind) {
                    out.push(GraphEdge {
                        from: from.clone(),
                        to: to.clone(),
                        kind: k,
                    });
                }
            }
        }
        out
    }

    pub fn ids_by_kind(&self, kind: NodeKind) -> Vec<String> {
        self.nodes
            .values()
            .filter(|n| n.kind == kind)
            .map(|n| n.id.clone())
            .collect()
    }

    /// Evidence/source/action nodes whose `resource_id` matches.
    pub fn nodes_for_resource(&self, resource_id: &str) -> Vec<&GraphNode> {
        self.nodes
            .values()
            .filter(|n| n.resource_id.as_deref() == Some(resource_id))
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImpactClassification {
    /// Dependents must be rechecked. Not a determination that they are false.
    RequiresVerification,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ImpactReason {
    pub node_id: String,
    pub resource_id: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, Default)]
pub struct ImpactReport {
    pub affected_decisions: Vec<String>,
    pub pending_actions: Vec<String>,
    pub predicates: Vec<String>,
    pub approvals: Vec<String>,
    pub invariants: Vec<String>,
    pub reasons: Vec<ImpactReason>,
    pub cyclic: bool,
    pub classification: ImpactClassification,
    pub duplicate_changes_collapsed: u32,
    pub missing_versions: Vec<String>,
    pub deleted_resources: Vec<String>,
    pub inaccessible: Vec<String>,
}

impl Default for ImpactClassification {
    fn default() -> Self {
        ImpactClassification::RequiresVerification
    }
}

/// Deterministic impact of one or more resource changes against an explicit graph.
///
/// Traversal is BFS over outgoing edges, visiting node ids in sorted order at each
/// frontier so concurrent changes produce the same report.
pub fn compute_impact(graph: &EvidenceGraph, changes: &[ResourceChange]) -> ImpactReport {
    let (merged, collapsed) = merge_changes(changes);
    let mut report = ImpactReport {
        classification: ImpactClassification::RequiresVerification,
        duplicate_changes_collapsed: collapsed,
        ..ImpactReport::default()
    };

    let mut seeds: BTreeSet<String> = BTreeSet::new();
    for change in &merged {
        if change.inaccessible {
            report.inaccessible.push(change.resource_id.clone());
            report.classification = ImpactClassification::Inconclusive;
        }
        if change.new_version.is_none() && !change.inaccessible {
            report.deleted_resources.push(change.resource_id.clone());
        }
        let matching: Vec<&GraphNode> = graph
            .nodes()
            .filter(|n| n.resource_id.as_deref() == Some(change.resource_id.as_str()))
            .collect();
        if matching.is_empty() {
            report.missing_versions.push(change.resource_id.clone());
            report.reasons.push(ImpactReason {
                node_id: change.resource_id.clone(),
                resource_id: change.resource_id.clone(),
                detail: "no registered evidence node for resource".into(),
            });
            continue;
        }
        let mut version_hit = false;
        for node in matching {
            let version_changed = match (&node.version, &change.new_version) {
                (Some(old), Some(new)) => old != new,
                (Some(_), None) => true,
                (None, _) => true,
            };
            if change.old_version.is_some() && node.version.as_ref() == change.old_version.as_ref()
            {
                version_hit = true;
            } else if change.old_version.is_none() {
                version_hit = true;
            }
            if version_changed || change.inaccessible {
                seeds.insert(node.id.clone());
                report.reasons.push(ImpactReason {
                    node_id: node.id.clone(),
                    resource_id: change.resource_id.clone(),
                    detail: reason_detail(change, node),
                });
            }
        }
        if !version_hit && change.old_version.is_some() {
            report.missing_versions.push(change.resource_id.clone());
            report.reasons.push(ImpactReason {
                node_id: change.resource_id.clone(),
                resource_id: change.resource_id.clone(),
                detail: format!(
                    "recorded version {} was not present in the graph",
                    change.old_version.as_deref().unwrap_or("?")
                ),
            });
            // Still seed any node for this resource so dependents are checked.
            for node in graph
                .nodes()
                .filter(|n| n.resource_id.as_deref() == Some(change.resource_id.as_str()))
            {
                seeds.insert(node.id.clone());
            }
        }
    }

    let (visited, cyclic) = walk_dependents(graph, &seeds);
    report.cyclic = cyclic;

    let mut decisions = BTreeSet::new();
    let mut actions = BTreeSet::new();
    let mut predicates = BTreeSet::new();
    let mut approvals = BTreeSet::new();
    let mut invariants = BTreeSet::new();
    for id in visited {
        if seeds.contains(&id) {
            // The changed evidence/source itself is not a dependent decision.
            if let Some(n) = graph.node(&id) {
                if n.kind != NodeKind::Decision
                    && n.kind != NodeKind::Action
                    && n.kind != NodeKind::Approval
                    && n.kind != NodeKind::GoalPredicate
                    && n.kind != NodeKind::Invariant
                {
                    continue;
                }
            }
        }
        match graph.node(&id).map(|n| n.kind) {
            Some(NodeKind::Decision) => {
                decisions.insert(id);
            }
            Some(NodeKind::Action) => {
                actions.insert(id);
            }
            Some(NodeKind::GoalPredicate) => {
                predicates.insert(id);
            }
            Some(NodeKind::Approval) => {
                approvals.insert(id);
            }
            Some(NodeKind::Invariant) => {
                invariants.insert(id);
            }
            _ => {}
        }
    }
    report.affected_decisions = decisions.into_iter().collect();
    report.pending_actions = actions.into_iter().collect();
    report.predicates = predicates.into_iter().collect();
    report.approvals = approvals.into_iter().collect();
    report.invariants = invariants.into_iter().collect();
    report
}

fn reason_detail(change: &ResourceChange, node: &GraphNode) -> String {
    if change.inaccessible {
        return format!("evidence {} is inaccessible", node.id);
    }
    match (&change.old_version, &change.new_version) {
        (Some(old), Some(new)) => {
            format!("resource {} changed {} → {}", change.resource_id, old, new)
        }
        (Some(old), None) => format!(
            "resource {} version {} was deleted",
            change.resource_id, old
        ),
        (None, Some(new)) => format!("resource {} observed at {}", change.resource_id, new),
        (None, None) => format!("resource {} changed (versions unknown)", change.resource_id),
    }
}

fn merge_changes(changes: &[ResourceChange]) -> (Vec<ResourceChange>, u32) {
    let mut by_id: BTreeMap<String, ResourceChange> = BTreeMap::new();
    let mut collapsed = 0u32;
    for c in changes {
        if by_id.contains_key(&c.resource_id) {
            collapsed += 1;
        }
        by_id.insert(
            c.resource_id.clone(),
            ResourceChange {
                resource_id: c.resource_id.clone(),
                old_version: c.old_version.clone().or_else(|| {
                    by_id
                        .get(&c.resource_id)
                        .and_then(|p| p.old_version.clone())
                }),
                new_version: c.new_version.clone(),
                inaccessible: c.inaccessible
                    || by_id
                        .get(&c.resource_id)
                        .map(|p| p.inaccessible)
                        .unwrap_or(false),
            },
        );
    }
    (by_id.into_values().collect(), collapsed)
}

/// BFS from seeds along outgoing edges. Returns visited ids (including seeds) and
/// whether a cycle was encountered. Cycles do not loop forever.
fn walk_dependents(graph: &EvidenceGraph, seeds: &BTreeSet<String>) -> (BTreeSet<String>, bool) {
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut cyclic = false;
    // On-stack set for cycle detection of the current BFS path is not unique in BFS;
    // we record a back-edge when a neighbor is already queued/visited *and* is an ancestor
    // via a grey set per walk. For DAG-or-cycle we use a rec-stack DFS from each seed
    // in sorted order, skipping already-finished nodes.
    let mut rec_stack: BTreeSet<String> = BTreeSet::new();
    let mut finished: BTreeSet<String> = BTreeSet::new();

    fn dfs(
        graph: &EvidenceGraph,
        id: &str,
        visited: &mut BTreeSet<String>,
        rec_stack: &mut BTreeSet<String>,
        finished: &mut BTreeSet<String>,
        cyclic: &mut bool,
    ) {
        if finished.contains(id) {
            return;
        }
        if rec_stack.contains(id) {
            *cyclic = true;
            return;
        }
        rec_stack.insert(id.to_string());
        visited.insert(id.to_string());
        if let Some(neigh) = graph.adj.get(id) {
            for (to, _) in neigh {
                dfs(graph, to, visited, rec_stack, finished, cyclic);
            }
        }
        rec_stack.remove(id);
        finished.insert(id.to_string());
    }

    for seed in seeds {
        dfs(
            graph,
            seed,
            &mut visited,
            &mut rec_stack,
            &mut finished,
            &mut cyclic,
        );
    }
    (visited, cyclic)
}

/// Observe `_resources` on a snapshot (or any JSON object) as current versions.
pub fn observed_resources(state: &serde_json::Value) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    if let Some(arr) = state.get("_resources").and_then(|v| v.as_array()) {
        for item in arr {
            let id = item
                .get("resource_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if id.is_empty() {
                continue;
            }
            let raw = item.get("resource_version").or_else(|| item.get("version"));
            let ver = raw.and_then(|v| {
                v.as_str()
                    .map(str::to_string)
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            });
            out.push((id.to_string(), ver));
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn parse_evidence_hints(value: &serde_json::Value) -> Vec<goshikq_protocol::EvidenceHint> {
    let arr = value
        .get("_evidence")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    arr.into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect()
}

pub fn parse_decision_hint(value: &serde_json::Value) -> Option<goshikq_protocol::DecisionHint> {
    value
        .get("_decision")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
}

/// Classify a revalidated decision. Missing current evidence is inconclusive, not invalid.
pub fn classify_revalidation(
    previous: DecisionStatus,
    current_version: Option<&str>,
    rule_passed: Option<bool>,
) -> DecisionStatus {
    let _ = previous;
    if current_version.is_none() {
        return DecisionStatus::Inconclusive;
    }
    match rule_passed {
        Some(true) => DecisionStatus::Valid,
        Some(false) => DecisionStatus::Invalid,
        None => DecisionStatus::Inconclusive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goshikq_protocol::ResourceChange;

    fn policy_graph() -> EvidenceGraph {
        let mut g = EvidenceGraph::new();
        g.insert_node(GraphNode {
            id: "ev:policy:POL-1@4".into(),
            kind: NodeKind::Evidence,
            resource_id: Some("policy:POL-1".into()),
            version: Some("4".into()),
            label: Some("policy v4".into()),
        });
        g.insert_node(GraphNode {
            id: "dec:coverage:CLM-77".into(),
            kind: NodeKind::Decision,
            resource_id: None,
            version: None,
            label: Some("coverage".into()),
        });
        g.insert_node(GraphNode {
            id: "dec:payout:CLM-77".into(),
            kind: NodeKind::Decision,
            resource_id: None,
            version: None,
            label: Some("payout recommendation".into()),
        });
        g.insert_node(GraphNode {
            id: "appr:payout:CLM-77".into(),
            kind: NodeKind::Approval,
            resource_id: None,
            version: None,
            label: Some("approval".into()),
        });
        g.insert_node(GraphNode {
            id: "act:authorize_payout".into(),
            kind: NodeKind::Action,
            resource_id: None,
            version: None,
            label: Some("authorize_payout".into()),
        });
        g.insert_node(GraphNode {
            id: "pred:claim_paid".into(),
            kind: NodeKind::GoalPredicate,
            resource_id: None,
            version: None,
            label: None,
        });
        g.insert_node(GraphNode {
            id: "inv:payout_within_reserve".into(),
            kind: NodeKind::Invariant,
            resource_id: None,
            version: None,
            label: None,
        });
        g.insert_edge(GraphEdge {
            from: "ev:policy:POL-1@4".into(),
            to: "dec:coverage:CLM-77".into(),
            kind: EdgeKind::Supports,
        });
        g.insert_edge(GraphEdge {
            from: "dec:coverage:CLM-77".into(),
            to: "dec:payout:CLM-77".into(),
            kind: EdgeKind::Supports,
        });
        g.insert_edge(GraphEdge {
            from: "dec:payout:CLM-77".into(),
            to: "appr:payout:CLM-77".into(),
            kind: EdgeKind::Authorizes,
        });
        g.insert_edge(GraphEdge {
            from: "dec:payout:CLM-77".into(),
            to: "act:authorize_payout".into(),
            kind: EdgeKind::Requires,
        });
        g.insert_edge(GraphEdge {
            from: "appr:payout:CLM-77".into(),
            to: "act:authorize_payout".into(),
            kind: EdgeKind::Authorizes,
        });
        g.insert_edge(GraphEdge {
            from: "act:authorize_payout".into(),
            to: "pred:claim_paid".into(),
            kind: EdgeKind::Requires,
        });
        g.insert_edge(GraphEdge {
            from: "dec:payout:CLM-77".into(),
            to: "inv:payout_within_reserve".into(),
            kind: EdgeKind::Requires,
        });
        g
    }

    #[test]
    fn policy_v4_to_v5_marks_dependents_for_verification() {
        let g = policy_graph();
        let report = compute_impact(
            &g,
            &[ResourceChange {
                resource_id: "policy:POL-1".into(),
                old_version: Some("4".into()),
                new_version: Some("5".into()),
                inaccessible: false,
            }],
        );
        assert!(!report.affected_decisions.is_empty());
        assert!(report
            .affected_decisions
            .contains(&"dec:coverage:CLM-77".into()));
        assert!(report
            .affected_decisions
            .contains(&"dec:payout:CLM-77".into()));
        assert!(report
            .pending_actions
            .contains(&"act:authorize_payout".into()));
        assert!(report.approvals.contains(&"appr:payout:CLM-77".into()));
        assert!(report.predicates.contains(&"pred:claim_paid".into()));
        assert_eq!(
            report.classification,
            ImpactClassification::RequiresVerification
        );
        assert!(!report.cyclic);
    }

    #[test]
    fn unrelated_decision_is_untouched() {
        let mut g = policy_graph();
        g.insert_node(GraphNode {
            id: "ev:order:ORD-1@3".into(),
            kind: NodeKind::Evidence,
            resource_id: Some("order:ORD-1".into()),
            version: Some("3".into()),
            label: None,
        });
        g.insert_node(GraphNode {
            id: "dec:eligibility:ORD-1".into(),
            kind: NodeKind::Decision,
            resource_id: None,
            version: None,
            label: None,
        });
        g.insert_edge(GraphEdge {
            from: "ev:order:ORD-1@3".into(),
            to: "dec:eligibility:ORD-1".into(),
            kind: EdgeKind::Supports,
        });
        let report = compute_impact(
            &g,
            &[ResourceChange {
                resource_id: "policy:POL-1".into(),
                old_version: Some("4".into()),
                new_version: Some("5".into()),
                inaccessible: false,
            }],
        );
        assert!(!report
            .affected_decisions
            .contains(&"dec:eligibility:ORD-1".into()));
    }

    #[test]
    fn cyclic_graph_terminates() {
        let mut g = EvidenceGraph::new();
        g.insert_node(GraphNode {
            id: "a".into(),
            kind: NodeKind::Evidence,
            resource_id: Some("r".into()),
            version: Some("1".into()),
            label: None,
        });
        g.insert_node(GraphNode {
            id: "b".into(),
            kind: NodeKind::Decision,
            resource_id: None,
            version: None,
            label: None,
        });
        g.insert_edge(GraphEdge {
            from: "a".into(),
            to: "b".into(),
            kind: EdgeKind::Supports,
        });
        g.insert_edge(GraphEdge {
            from: "b".into(),
            to: "a".into(),
            kind: EdgeKind::Requires,
        });
        let report = compute_impact(
            &g,
            &[ResourceChange {
                resource_id: "r".into(),
                old_version: Some("1".into()),
                new_version: Some("2".into()),
                inaccessible: false,
            }],
        );
        assert!(report.cyclic);
        assert!(report.affected_decisions.contains(&"b".into()));
    }

    #[test]
    fn missing_version_is_inconclusive_seed() {
        let g = policy_graph();
        let report = compute_impact(
            &g,
            &[ResourceChange {
                resource_id: "policy:POL-1".into(),
                old_version: Some("99".into()),
                new_version: Some("100".into()),
                inaccessible: false,
            }],
        );
        assert!(report.missing_versions.contains(&"policy:POL-1".into()));
        assert!(!report.affected_decisions.is_empty());
    }

    #[test]
    fn deleted_resource_flags_dependents() {
        let g = policy_graph();
        let report = compute_impact(
            &g,
            &[ResourceChange {
                resource_id: "policy:POL-1".into(),
                old_version: Some("4".into()),
                new_version: None,
                inaccessible: false,
            }],
        );
        assert!(report.deleted_resources.contains(&"policy:POL-1".into()));
        assert!(report
            .pending_actions
            .contains(&"act:authorize_payout".into()));
    }

    #[test]
    fn inaccessible_evidence_is_inconclusive() {
        let g = policy_graph();
        let report = compute_impact(
            &g,
            &[ResourceChange {
                resource_id: "policy:POL-1".into(),
                old_version: Some("4".into()),
                new_version: Some("4".into()),
                inaccessible: true,
            }],
        );
        assert_eq!(report.classification, ImpactClassification::Inconclusive);
        assert!(report.inaccessible.contains(&"policy:POL-1".into()));
    }

    #[test]
    fn duplicate_changes_collapse() {
        let g = policy_graph();
        let c = ResourceChange {
            resource_id: "policy:POL-1".into(),
            old_version: Some("4".into()),
            new_version: Some("5".into()),
            inaccessible: false,
        };
        let a = compute_impact(
            &g,
            &[
                c.clone(),
                c.clone(),
                ResourceChange {
                    new_version: Some("6".into()),
                    ..c
                },
            ],
        );
        assert_eq!(a.duplicate_changes_collapsed, 2);
        let b = compute_impact(
            &g,
            &[ResourceChange {
                resource_id: "policy:POL-1".into(),
                old_version: Some("4".into()),
                new_version: Some("6".into()),
                inaccessible: false,
            }],
        );
        assert_eq!(a.affected_decisions, b.affected_decisions);
        assert_eq!(a.pending_actions, b.pending_actions);
    }

    #[test]
    fn simultaneous_changes_are_deterministic() {
        let mut g = policy_graph();
        g.insert_node(GraphNode {
            id: "ev:order:ORD-1@3".into(),
            kind: NodeKind::Evidence,
            resource_id: Some("order:ORD-1".into()),
            version: Some("3".into()),
            label: None,
        });
        g.insert_node(GraphNode {
            id: "dec:eligibility:ORD-1".into(),
            kind: NodeKind::Decision,
            resource_id: None,
            version: None,
            label: None,
        });
        g.insert_edge(GraphEdge {
            from: "ev:order:ORD-1@3".into(),
            to: "dec:eligibility:ORD-1".into(),
            kind: EdgeKind::Supports,
        });
        let c1 = ResourceChange {
            resource_id: "policy:POL-1".into(),
            old_version: Some("4".into()),
            new_version: Some("5".into()),
            inaccessible: false,
        };
        let c2 = ResourceChange {
            resource_id: "order:ORD-1".into(),
            old_version: Some("3".into()),
            new_version: Some("4".into()),
            inaccessible: false,
        };
        let a = compute_impact(&g, &[c1.clone(), c2.clone()]);
        let b = compute_impact(&g, &[c2, c1]);
        assert_eq!(a.affected_decisions, b.affected_decisions);
        assert!(a.affected_decisions.contains(&"dec:coverage:CLM-77".into()));
        assert!(a
            .affected_decisions
            .contains(&"dec:eligibility:ORD-1".into()));
    }

    #[test]
    fn unknown_resource_does_not_panic() {
        let g = policy_graph();
        let report = compute_impact(
            &g,
            &[ResourceChange {
                resource_id: "missing:X".into(),
                old_version: Some("1".into()),
                new_version: Some("2".into()),
                inaccessible: false,
            }],
        );
        assert!(report.affected_decisions.is_empty());
        assert!(report.missing_versions.contains(&"missing:X".into()));
    }

    #[test]
    fn classify_missing_is_inconclusive() {
        assert_eq!(
            classify_revalidation(DecisionStatus::Valid, None, Some(false)),
            DecisionStatus::Inconclusive
        );
        assert_eq!(
            classify_revalidation(DecisionStatus::PendingRevalidation, Some("5"), Some(true)),
            DecisionStatus::Valid
        );
        assert_eq!(
            classify_revalidation(DecisionStatus::PendingRevalidation, Some("5"), Some(false)),
            DecisionStatus::Invalid
        );
    }

    #[test]
    fn observed_resources_reads_snapshot_hint() {
        let v = serde_json::json!({
            "_resources": [
                {"resource_id": "policy:POL-1", "version": "5"},
                {"resource_id": "order:ORD-1", "resource_version": "4"},
            ]
        });
        let got = observed_resources(&v);
        assert_eq!(
            got,
            vec![
                ("order:ORD-1".into(), Some("4".into())),
                ("policy:POL-1".into(), Some("5".into()))
            ]
        );
    }
}
