//! Seeded, reproducible evidence-graph scenarios.
//!
//! The **true** graph may contain hidden edges that are withheld from algorithms
//! under test. Ground truth is computed from the true graph in `ground_truth.rs`,
//! not from `goshikq_core::compute_impact`.

use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use goshikq_core::{EvidenceGraph, GraphEdge, GraphNode};
use goshikq_protocol::{
    EdgeKind, EvidenceCoverageContract, EvidenceFact, MissingEvidenceBehavior, NodeKind,
    RequiredEvidence, ResourceChange, RiskLevel, TrustClassification,
};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioFamily {
    UnrelatedChange,
    MultipleSimultaneous,
    DeletedEvidence,
    InaccessibleEvidence,
    MissingVersions,
    ConcurrentChanges,
    ApprovalDelay,
    ExternalOperationDelay,
    DuplicateChangeEvents,
    CyclicGraphAttempt,
    MissingRegisteredDependencies,
    HiddenDependency,
}

impl ScenarioFamily {
    pub const ALL: [ScenarioFamily; 12] = [
        ScenarioFamily::UnrelatedChange,
        ScenarioFamily::MultipleSimultaneous,
        ScenarioFamily::DeletedEvidence,
        ScenarioFamily::InaccessibleEvidence,
        ScenarioFamily::MissingVersions,
        ScenarioFamily::ConcurrentChanges,
        ScenarioFamily::ApprovalDelay,
        ScenarioFamily::ExternalOperationDelay,
        ScenarioFamily::DuplicateChangeEvents,
        ScenarioFamily::CyclicGraphAttempt,
        ScenarioFamily::MissingRegisteredDependencies,
        ScenarioFamily::HiddenDependency,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ScenarioFamily::UnrelatedChange => "unrelated_change",
            ScenarioFamily::MultipleSimultaneous => "multiple_simultaneous",
            ScenarioFamily::DeletedEvidence => "deleted_evidence",
            ScenarioFamily::InaccessibleEvidence => "inaccessible_evidence",
            ScenarioFamily::MissingVersions => "missing_versions",
            ScenarioFamily::ConcurrentChanges => "concurrent_changes",
            ScenarioFamily::ApprovalDelay => "approval_delay",
            ScenarioFamily::ExternalOperationDelay => "external_operation_delay",
            ScenarioFamily::DuplicateChangeEvents => "duplicate_change_events",
            ScenarioFamily::CyclicGraphAttempt => "cyclic_graph_attempt",
            ScenarioFamily::MissingRegisteredDependencies => "missing_registered_dependencies",
            ScenarioFamily::HiddenDependency => "hidden_dependency",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub family: ScenarioFamily,
    pub seed: u64,
    pub size: usize,
    pub nodes: Vec<GraphNode>,
    pub declared_edges: Vec<GraphEdge>,
    pub hidden_edges: Vec<GraphEdge>,
    pub changes: Vec<ResourceChange>,
    /// Metadata only. Algorithms are not wall-clock schedulers.
    pub approval_delay_ms: u32,
    /// Metadata only. Algorithms are not wall-clock schedulers.
    pub external_delay_ms: u32,
    /// Per-action declared coverage contracts (used by STRICT_CONTRACT only).
    #[serde(default)]
    pub action_contracts: BTreeMap<String, EvidenceCoverageContract>,
    /// Recorded evidence facts (used by STRICT_CONTRACT only).
    #[serde(default)]
    pub facts: Vec<EvidenceFact>,
}

impl Scenario {
    pub fn declared_graph(&self) -> EvidenceGraph {
        let mut g = EvidenceGraph::new();
        for n in &self.nodes {
            g.insert_node(n.clone());
        }
        for e in &self.declared_edges {
            g.insert_edge(e.clone());
        }
        g
    }

    pub fn true_adj(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .declared_edges
            .iter()
            .map(|e| (e.from.clone(), e.to.clone()))
            .chain(
                self.hidden_edges
                    .iter()
                    .map(|e| (e.from.clone(), e.to.clone())),
            )
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

fn node(
    id: impl Into<String>,
    kind: NodeKind,
    resource_id: Option<String>,
    version: Option<String>,
) -> GraphNode {
    GraphNode {
        id: id.into(),
        kind,
        resource_id,
        version,
        label: None,
    }
}

/// Deterministic scenario for `(seed, size, family)`.
pub fn generate(seed: u64, size: usize, family: ScenarioFamily) -> Scenario {
    assert!(
        size >= 4,
        "need at least 4 nodes to form evidence/decision/action/approval"
    );
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    let n_evidence = size;
    let n_decisions = (size / 2).max(2);
    let n_actions = (size / 4).max(2);
    let n_approvals = (size / 8).max(1);

    let mut nodes = Vec::new();
    for i in 0..n_evidence {
        nodes.push(node(
            format!("ev-{i:05}"),
            NodeKind::Evidence,
            Some(format!("res:{i:05}")),
            Some("1".into()),
        ));
    }
    for i in 0..n_decisions {
        nodes.push(node(format!("dec-{i:05}"), NodeKind::Decision, None, None));
    }
    for i in 0..n_actions {
        // Even-indexed actions declare the same resource as evidence i (conventional can see them).
        // Odd-indexed actions declare no resource (only a graph walk can reach them).
        let rid = if i % 2 == 0 {
            Some(format!("res:{:05}", i % n_evidence))
        } else {
            None
        };
        let ver = rid.as_ref().map(|_| "1".to_string());
        nodes.push(node(format!("act-{i:05}"), NodeKind::Action, rid, ver));
    }
    for i in 0..n_approvals {
        nodes.push(node(format!("appr-{i:05}"), NodeKind::Approval, None, None));
    }

    let mut declared_edges = Vec::new();
    let mut hidden_edges = Vec::new();

    // Declared: evidence i → decision (i % n_decisions), then decision → a matching action.
    for i in 0..n_evidence {
        let dec = i % n_decisions;
        declared_edges.push(GraphEdge {
            from: format!("ev-{i:05}"),
            to: format!("dec-{dec:05}"),
            kind: EdgeKind::Supports,
        });
    }
    for i in 0..n_decisions {
        let act = i % n_actions;
        declared_edges.push(GraphEdge {
            from: format!("dec-{i:05}"),
            to: format!("act-{act:05}"),
            kind: EdgeKind::Requires,
        });
        let appr = i % n_approvals;
        declared_edges.push(GraphEdge {
            from: format!("dec-{i:05}"),
            to: format!("appr-{appr:05}"),
            kind: EdgeKind::Authorizes,
        });
    }

    // A few extra declared edges so the graph is not a pure tree.
    for _ in 0..(size / 10).max(1) {
        let a = rng.gen_range(0..n_evidence);
        let b = rng.gen_range(0..n_decisions);
        declared_edges.push(GraphEdge {
            from: format!("ev-{a:05}"),
            to: format!("dec-{b:05}"),
            kind: EdgeKind::Supports,
        });
    }

    declared_edges
        .sort_by(|x, y| (&x.from, &x.to, x.kind.as_str()).cmp(&(&y.from, &y.to, y.kind.as_str())));
    declared_edges.dedup();

    let declared_pairs: std::collections::HashSet<(String, String)> = declared_edges
        .iter()
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();

    // Hidden edges: evidence → a decision that is *not* a declared target of that evidence.
    // Algorithms that only see declared edges cannot reach these dependents.
    let n_hidden = ((size / 10) as usize).max(1).min(n_evidence);
    for k in 0..n_hidden {
        let src = k;
        let from = format!("ev-{src:05}");
        let mut dest = (k + n_decisions / 2 + 1) % n_decisions;
        let mut placed = false;
        for _ in 0..n_decisions {
            let to = format!("dec-{dest:05}");
            if !declared_pairs.contains(&(from.clone(), to.clone())) {
                hidden_edges.push(GraphEdge {
                    from: from.clone(),
                    to,
                    kind: EdgeKind::Supports,
                });
                placed = true;
                break;
            }
            dest = (dest + 1) % n_decisions;
        }
        let _ = placed;
    }

    let mut changes = Vec::new();
    let mut approval_delay_ms = 0u32;
    let mut external_delay_ms = 0u32;

    match family {
        ScenarioFamily::UnrelatedChange => {
            // Change an evidence node that has no hidden extra dependents beyond its primary chain
            // PLUS a synthetic isolated resource that no decision uses.
            nodes.push(node(
                "ev-isolated",
                NodeKind::Evidence,
                Some("res:isolated".into()),
                Some("1".into()),
            ));
            changes.push(ResourceChange {
                resource_id: "res:isolated".into(),
                old_version: Some("1".into()),
                new_version: Some("2".into()),
                inaccessible: false,
            });
        }
        ScenarioFamily::MultipleSimultaneous | ScenarioFamily::ConcurrentChanges => {
            for k in 0..3.min(n_evidence) {
                changes.push(ResourceChange {
                    resource_id: format!("res:{k:05}"),
                    old_version: Some("1".into()),
                    new_version: Some("2".into()),
                    inaccessible: false,
                });
            }
        }
        ScenarioFamily::DeletedEvidence => {
            changes.push(ResourceChange {
                resource_id: "res:00000".into(),
                old_version: Some("1".into()),
                new_version: None,
                inaccessible: false,
            });
        }
        ScenarioFamily::InaccessibleEvidence => {
            changes.push(ResourceChange {
                resource_id: "res:00000".into(),
                old_version: Some("1".into()),
                new_version: Some("1".into()),
                inaccessible: true,
            });
        }
        ScenarioFamily::MissingVersions => {
            changes.push(ResourceChange {
                resource_id: "res:00000".into(),
                old_version: Some("99".into()),
                new_version: Some("100".into()),
                inaccessible: false,
            });
        }
        ScenarioFamily::ApprovalDelay => {
            approval_delay_ms = 250;
            changes.push(hit_primary());
        }
        ScenarioFamily::ExternalOperationDelay => {
            external_delay_ms = 400;
            changes.push(hit_primary());
        }
        ScenarioFamily::DuplicateChangeEvents => {
            let c = hit_primary();
            changes.push(c.clone());
            changes.push(c);
        }
        ScenarioFamily::CyclicGraphAttempt => {
            declared_edges.push(GraphEdge {
                from: "dec-00000".into(),
                to: "ev-00000".into(),
                kind: EdgeKind::Invalidates,
            });
            changes.push(hit_primary());
        }
        ScenarioFamily::MissingRegisteredDependencies => {
            changes.push(ResourceChange {
                resource_id: "res:never-registered".into(),
                old_version: Some("1".into()),
                new_version: Some("2".into()),
                inaccessible: false,
            });
        }
        ScenarioFamily::HiddenDependency => {
            changes.push(hit_primary());
        }
    }

    let mut action_contracts = BTreeMap::new();
    for i in 0..n_actions {
        let id = format!("act-{i:05}");
        let mut required = Vec::new();
        if i % 2 == 0 {
            required.push(RequiredEvidence::from_key(format!(
                "res:{:05}",
                i % n_evidence
            )));
        }
        if i % 4 == 1 {
            // Developer-declared requirement for a resource the graph does not attach
            // to this action. STRICT can block it; selective cannot via the hidden edge.
            required.push(RequiredEvidence::from_key("res:00000"));
        }
        if !required.is_empty() {
            action_contracts.insert(
                id,
                EvidenceCoverageContract {
                    required,
                    required_invariants: vec![],
                    required_authorization: vec![],
                    on_missing: MissingEvidenceBehavior::Block,
                    risk: RiskLevel::High,
                },
            );
        }
    }

    let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let mut facts = Vec::new();
    for n in &nodes {
        if n.kind != NodeKind::Evidence {
            continue;
        }
        let rid = n.resource_id.clone().unwrap_or_default();
        let current = changes
            .iter()
            .find(|c| c.resource_id == rid)
            .map(|c| c.new_version.clone())
            .unwrap_or_else(|| n.version.clone());
        facts.push(EvidenceFact {
            id: n.id.clone(),
            resource_id: rid,
            keys: vec![],
            version: n.version.clone(),
            trust: TrustClassification::Authoritative,
            retrieved_at: now,
            current_version: current,
            inaccessible: changes
                .iter()
                .any(|c| c.resource_id == n.resource_id.as_deref().unwrap_or("") && c.inaccessible),
            fingerprint: n.version.as_ref().map(|v| format!("fp-{v}")),
            source_system: Some("bench".into()),
        });
    }

    Scenario {
        id: format!("{size}-{}-{seed}", family.as_str()),
        family,
        seed,
        size,
        nodes,
        declared_edges,
        hidden_edges,
        changes,
        approval_delay_ms,
        external_delay_ms,
        action_contracts,
        facts,
    }
}

fn hit_primary() -> ResourceChange {
    ResourceChange {
        resource_id: "res:00000".into(),
        old_version: Some("1".into()),
        new_version: Some("2".into()),
        inaccessible: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_is_deterministic() {
        let a = generate(42, 20, ScenarioFamily::HiddenDependency);
        let b = generate(42, 20, ScenarioFamily::HiddenDependency);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }

    #[test]
    fn different_seeds_differ() {
        let a = generate(1, 20, ScenarioFamily::HiddenDependency);
        let b = generate(2, 20, ScenarioFamily::HiddenDependency);
        assert_ne!(a.declared_edges, b.declared_edges);
    }

    #[test]
    fn hidden_edges_are_withheld_from_declared_graph() {
        let s = generate(7, 20, ScenarioFamily::HiddenDependency);
        assert!(!s.hidden_edges.is_empty());
        let g = s.declared_graph();
        let declared: Vec<_> = g.edges();
        for h in &s.hidden_edges {
            assert!(
                !declared.iter().any(|e| e.from == h.from && e.to == h.to),
                "hidden edge leaked into declared graph"
            );
        }
    }
}
