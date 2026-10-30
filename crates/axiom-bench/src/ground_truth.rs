//! Independent ground-truth construction.
//!
//! Walks the **true** adjacency (declared ∪ hidden) with a BFS that does **not**
//! call `axiom_core::compute_impact`. Hidden edges are the reason selective
//! revalidation can miss dependents: the algorithm is not given those edges.

use std::collections::{HashMap, HashSet, VecDeque};

use axiom_core::GraphNode;
use axiom_protocol::{NodeKind, ResourceChange};

use crate::generate::Scenario;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AffectedSet {
    pub decisions: Vec<String>,
    pub actions: Vec<String>,
    pub approvals: Vec<String>,
}

impl AffectedSet {
    pub fn all_ids(&self) -> Vec<String> {
        let mut v = Vec::new();
        v.extend(self.decisions.iter().cloned());
        v.extend(self.actions.iter().cloned());
        v.extend(self.approvals.iter().cloned());
        v.sort();
        v.dedup();
        v
    }

    pub fn is_empty(&self) -> bool {
        self.decisions.is_empty() && self.actions.is_empty() && self.approvals.is_empty()
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

/// BFS over true edges (declared ∪ hidden). Does not call `compute_impact`.
pub fn affected(scenario: &Scenario) -> AffectedSet {
    walk(scenario, scenario.true_adj())
}

/// BFS over **declared** edges only. Measures registered-dependency detection.
pub fn affected_declared(scenario: &Scenario) -> AffectedSet {
    let edges: Vec<(String, String)> = scenario
        .declared_edges
        .iter()
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    walk(scenario, edges)
}

/// Nodes in the true walk that the declared walk cannot reach.
pub fn hidden_only(true_set: &AffectedSet, registered: &AffectedSet) -> AffectedSet {
    let reg = registered.all_ids();
    let filter = |ids: &[String]| ids.iter().filter(|id| !reg.contains(id)).cloned().collect();
    AffectedSet {
        decisions: filter(&true_set.decisions),
        actions: filter(&true_set.actions),
        approvals: filter(&true_set.approvals),
    }
}

fn walk(scenario: &Scenario, edges: Vec<(String, String)>) -> AffectedSet {
    let mut by_id: HashMap<&str, &GraphNode> = HashMap::new();
    for n in &scenario.nodes {
        by_id.insert(n.id.as_str(), n);
    }

    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for (from, to) in edges {
        adj.entry(from).or_default().push(to);
    }
    for v in adj.values_mut() {
        v.sort();
        v.dedup();
    }

    let mut seeds: HashSet<String> = HashSet::new();
    for change in &scenario.changes {
        for n in &scenario.nodes {
            if n.resource_id.as_deref() == Some(change.resource_id.as_str())
                && change_hits(change, n.version.as_deref())
            {
                seeds.insert(n.id.clone());
            }
        }
    }

    let mut start: Vec<String> = seeds.into_iter().collect();
    start.sort();
    let mut q: VecDeque<String> = start.into();
    let mut visited: HashSet<String> = HashSet::new();

    while let Some(id) = q.pop_front() {
        if !visited.insert(id.clone()) {
            continue;
        }
        if let Some(neigh) = adj.get(&id) {
            for to in neigh {
                if !visited.contains(to) {
                    q.push_back(to.clone());
                }
            }
        }
    }

    let mut decisions = Vec::new();
    let mut actions = Vec::new();
    let mut approvals = Vec::new();
    for id in &visited {
        match by_id.get(id.as_str()).map(|n| n.kind) {
            Some(NodeKind::Decision) => decisions.push(id.clone()),
            Some(NodeKind::Action) => actions.push(id.clone()),
            Some(NodeKind::Approval) => approvals.push(id.clone()),
            _ => {}
        }
    }
    decisions.sort();
    actions.sort();
    approvals.sort();
    AffectedSet {
        decisions,
        actions,
        approvals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{generate, ScenarioFamily};
    use axiom_core::{run_strategy, RevalidationStrategy};

    #[test]
    fn hidden_dependents_are_in_ground_truth_not_selective() {
        let s = generate(7, 32, ScenarioFamily::HiddenDependency);
        let gt = affected(&s);
        let declared = s.declared_graph();
        let d = run_strategy(RevalidationStrategy::Selective, &declared, &s.changes);
        assert!(!gt.decisions.is_empty(), "true graph must have affected decisions");
        let extra: Vec<_> = gt
            .decisions
            .iter()
            .filter(|id| !d.decisions_to_recheck.contains(id))
            .cloned()
            .collect();
        assert!(
            !extra.is_empty(),
            "hidden edges must produce at least one decision that selective cannot see"
        );
    }

    #[test]
    fn unrelated_isolated_resource_has_empty_ground_truth() {
        let s = generate(3, 16, ScenarioFamily::UnrelatedChange);
        let gt = affected(&s);
        assert!(gt.is_empty(), "isolated resource change must not affect decisions");
    }

    #[test]
    fn missing_registered_resource_has_empty_ground_truth() {
        let s = generate(3, 16, ScenarioFamily::MissingRegisteredDependencies);
        let gt = affected(&s);
        assert!(gt.is_empty());
    }
}
