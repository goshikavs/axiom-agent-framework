//! Confusion-matrix metrics with **labeled denominators**.
//!
//! v0.3.1 reported a single "recall" that mixed four questions:
//! 1. Detection of all true affected dependencies (true-graph walk, includes hidden).
//! 2. Detection of registered dependencies only (declared-graph walk).
//! 3. Selection of all declared decisions (full-declared trivially scores 1 on #1).
//! 4. Prevention of incorrect actions (action-level false negatives).
//!
//! Full-declared 100% true-impact recall is **not** detection of hidden edges. Hidden
//! edges in the generator land on existing declared node ids; selecting every declared
//! node covers those ids. That is flagged as `trivial_full_selection`.

use serde::{Deserialize, Serialize};

use axiom_core::{EvidenceGraph, StrategyOutput};
use axiom_protocol::NodeKind;

use crate::ground_truth::{hidden_only, AffectedSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Confusion {
    pub true_positive: usize,
    pub false_positive: usize,
    pub false_negative: usize,
    pub precision: f64,
    pub recall: f64,
    /// True when the truth set is empty (tp+fn == 0). Recall is then 1.0 by convention
    /// and must not be averaged as "detected dependents."
    pub vacuous: bool,
}

impl Confusion {
    pub fn from_sets(predicted: &[String], truth: &[String]) -> Self {
        let pred: std::collections::BTreeSet<&String> = predicted.iter().collect();
        let gt: std::collections::BTreeSet<&String> = truth.iter().collect();
        let tp = pred.intersection(&gt).count();
        let fp = pred.difference(&gt).count();
        let fn_ = gt.difference(&pred).count();
        let vacuous = tp + fn_ == 0;
        let precision = if tp + fp == 0 { 1.0 } else { tp as f64 / (tp + fp) as f64 };
        let recall = if vacuous { 1.0 } else { tp as f64 / (tp + fn_) as f64 };
        Self {
            true_positive: tp,
            false_positive: fp,
            false_negative: fn_,
            precision,
            recall,
            vacuous,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialMetrics {
    pub strategy: String,
    pub predicted_decisions: usize,
    pub predicted_actions: usize,
    pub predicted_approvals: usize,
    pub ground_truth_decisions: usize,
    pub ground_truth_actions: usize,
    pub ground_truth_approvals: usize,
    /// Legacy alias of `true_impact` (v0.3.1 field). Prefer labeled fields.
    pub union: Confusion,
    pub decisions: Confusion,
    pub actions: Confusion,
    pub true_impact: Confusion,
    pub registered_impact: Confusion,
    pub hidden_only: Confusion,
    pub missed_critical_actions: usize,
    pub missed_critical_decisions: usize,
    pub false_positive_revalidation: usize,
    pub incorrect_action: u8,
    pub false_blocks: usize,
    pub human_review: u8,
    pub verification_workload: usize,
    pub latency_ns: u128,
    pub selects_all_declared_decisions: bool,
    pub selects_all_declared_actions: bool,
    /// True when the strategy selected every declared decision and action. True-impact
    /// recall of 1.0 in this case is not hidden-edge detection.
    pub trivial_full_selection: bool,
    pub hidden_only_count: usize,
    pub coverage_blocked_extra: usize,
}

pub fn evaluate(
    strategy_name: &str,
    predicted: &StrategyOutput,
    truth: &AffectedSet,
    registered: &AffectedSet,
    declared: &EvidenceGraph,
    latency_ns: u128,
) -> TrialMetrics {
    let pred_all = {
        let mut v = predicted.decisions_to_recheck.clone();
        v.extend(predicted.actions_to_block.iter().cloned());
        v.extend(predicted.approvals_to_invalidate.iter().cloned());
        v.sort();
        v.dedup();
        v
    };
    let gt_all = truth.all_ids();
    let reg_all = registered.all_ids();
    let hidden = hidden_only(truth, registered);
    let true_impact = Confusion::from_sets(&pred_all, &gt_all);
    let registered_impact = Confusion::from_sets(&pred_all, &reg_all);
    let hidden_c = Confusion::from_sets(&pred_all, &hidden.all_ids());
    let decisions = Confusion::from_sets(&predicted.decisions_to_recheck, &truth.decisions);
    let actions = Confusion::from_sets(&predicted.actions_to_block, &truth.actions);
    let false_positive_revalidation = true_impact.false_positive;
    let missed_critical_decisions = decisions.false_negative;
    let all_dec = declared.ids_by_kind(NodeKind::Decision);
    let all_act = declared.ids_by_kind(NodeKind::Action);
    let selects_all_declared_decisions = !all_dec.is_empty()
        && all_dec.iter().all(|id| predicted.decisions_to_recheck.contains(id));
    let selects_all_declared_actions =
        !all_act.is_empty() && all_act.iter().all(|id| predicted.actions_to_block.contains(id));
    let trivial_full_selection = selects_all_declared_decisions && selects_all_declared_actions;
    TrialMetrics {
        strategy: strategy_name.to_string(),
        predicted_decisions: predicted.decisions_to_recheck.len(),
        predicted_actions: predicted.actions_to_block.len(),
        predicted_approvals: predicted.approvals_to_invalidate.len(),
        ground_truth_decisions: truth.decisions.len(),
        ground_truth_actions: truth.actions.len(),
        ground_truth_approvals: truth.approvals.len(),
        union: true_impact.clone(),
        decisions,
        actions: actions.clone(),
        true_impact,
        registered_impact,
        hidden_only: hidden_c,
        missed_critical_actions: actions.false_negative,
        missed_critical_decisions,
        false_positive_revalidation,
        incorrect_action: if actions.false_negative > 0 { 1 } else { 0 },
        false_blocks: actions.false_positive,
        human_review: if predicted.approvals_to_invalidate.is_empty() { 0 } else { 1 },
        verification_workload: pred_all.len(),
        latency_ns,
        selects_all_declared_decisions,
        selects_all_declared_actions,
        trivial_full_selection,
        hidden_only_count: hidden.all_ids().len(),
        coverage_blocked_extra: 0,
    }
}

/// Fail if a reported 1.0 true-impact recall would be mistaken for hidden-edge detection.
pub fn honesty_warnings(m: &TrialMetrics) -> Vec<String> {
    let mut w = Vec::new();
    if m.true_impact.vacuous {
        w.push("true_impact.recall is vacuous (empty ground truth); do not treat as detection".into());
    }
    if m.trivial_full_selection && m.hidden_only_count > 0 && (m.true_impact.recall - 1.0).abs() < 1e-9 {
        w.push(
            "true_impact.recall=1.0 is trivial_full_selection: the strategy selected every declared node, not hidden edges"
                .into(),
        );
    }
    if m.hidden_only.vacuous == false && m.hidden_only.recall == 0.0 {
        w.push("hidden_only.recall=0: strategy did not select any node reachable only via hidden edges".into());
    }
    w
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregate {
    pub n: usize,
    pub mean: f64,
    pub stddev: f64,
    pub ci95_low: Option<f64>,
    pub ci95_high: Option<f64>,
}

impl Aggregate {
    pub fn of(values: &[f64]) -> Self {
        let n = values.len();
        if n == 0 {
            return Self {
                n: 0,
                mean: 0.0,
                stddev: 0.0,
                ci95_low: None,
                ci95_high: None,
            };
        }
        let mean = values.iter().sum::<f64>() / n as f64;
        let var = if n > 1 {
            values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0)
        } else {
            0.0
        };
        let stddev = var.sqrt();
        let (ci95_low, ci95_high) = if n > 1 {
            let se = stddev / (n as f64).sqrt();
            let half = 1.96 * se;
            (Some(mean - half), Some(mean + half))
        } else {
            (None, None)
        };
        Self {
            n,
            mean,
            stddev,
            ci95_low,
            ci95_high,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySummary {
    pub strategy: String,
    /// Unlabeled v0.3.1 field: true-impact recall including vacuous trials.
    pub recall: Aggregate,
    pub precision: Aggregate,
    pub missed_critical_action_rate: Aggregate,
    pub false_positive_revalidation_rate: Aggregate,
    pub incorrect_action_rate: Aggregate,
    pub verification_workload: Aggregate,
    pub latency_ns: Aggregate,
    pub true_impact_recall: Aggregate,
    pub true_impact_recall_nonvacuous: Aggregate,
    pub registered_impact_recall: Aggregate,
    pub hidden_detection_recall_nonvacuous: Aggregate,
    pub action_prevention_recall_nonvacuous: Aggregate,
    pub false_block_rate: Aggregate,
    pub human_review_rate: Aggregate,
    pub trivial_full_selection_rate: Aggregate,
}

pub fn summarize(strategy: &str, trials: &[TrialMetrics]) -> StrategySummary {
    let recall: Vec<f64> = trials.iter().map(|t| t.union.recall).collect();
    let precision: Vec<f64> = trials.iter().map(|t| t.union.precision).collect();
    let missed: Vec<f64> = trials.iter().map(|t| t.incorrect_action as f64).collect();
    let fp_rate: Vec<f64> = trials
        .iter()
        .map(|t| {
            let denom = t.verification_workload.max(1);
            t.false_positive_revalidation as f64 / denom as f64
        })
        .collect();
    let workload: Vec<f64> = trials.iter().map(|t| t.verification_workload as f64).collect();
    let latency: Vec<f64> = trials.iter().map(|t| t.latency_ns as f64).collect();
    let nonvac: Vec<f64> = trials
        .iter()
        .filter(|t| !t.true_impact.vacuous)
        .map(|t| t.true_impact.recall)
        .collect();
    let hidden: Vec<f64> = trials
        .iter()
        .filter(|t| !t.hidden_only.vacuous)
        .map(|t| t.hidden_only.recall)
        .collect();
    let act_prev: Vec<f64> = trials
        .iter()
        .filter(|t| !t.actions.vacuous)
        .map(|t| t.actions.recall)
        .collect();
    let false_block: Vec<f64> = trials
        .iter()
        .map(|t| {
            if t.predicted_actions == 0 {
                0.0
            } else {
                t.false_blocks as f64 / t.predicted_actions as f64
            }
        })
        .collect();
    StrategySummary {
        strategy: strategy.to_string(),
        recall: Aggregate::of(&recall),
        precision: Aggregate::of(&precision),
        missed_critical_action_rate: Aggregate::of(&missed),
        false_positive_revalidation_rate: Aggregate::of(&fp_rate),
        incorrect_action_rate: Aggregate::of(&missed),
        verification_workload: Aggregate::of(&workload),
        latency_ns: Aggregate::of(&latency),
        true_impact_recall: Aggregate::of(&recall),
        true_impact_recall_nonvacuous: Aggregate::of(&nonvac),
        registered_impact_recall: Aggregate::of(&trials.iter().map(|t| t.registered_impact.recall).collect::<Vec<_>>()),
        hidden_detection_recall_nonvacuous: Aggregate::of(&hidden),
        action_prevention_recall_nonvacuous: Aggregate::of(&act_prev),
        false_block_rate: Aggregate::of(&false_block),
        human_review_rate: Aggregate::of(&trials.iter().map(|t| t.human_review as f64).collect::<Vec<_>>()),
        trivial_full_selection_rate: Aggregate::of(
            &trials.iter().map(|t| if t.trivial_full_selection { 1.0 } else { 0.0 }).collect::<Vec<_>>(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{generate, ScenarioFamily};
    use crate::ground_truth::{affected, affected_declared};
    use axiom_core::{run_strategy, RevalidationStrategy};

    #[test]
    fn full_declared_recall_is_trivial_on_hidden_edges() {
        let s = generate(7, 32, ScenarioFamily::HiddenDependency);
        let declared = s.declared_graph();
        let pred = run_strategy(RevalidationStrategy::FullDeclared, &declared, &s.changes);
        let m = evaluate(
            "full_declared",
            &pred,
            &affected(&s),
            &affected_declared(&s),
            &declared,
            0,
        );
        assert!((m.true_impact.recall - 1.0).abs() < 1e-9);
        assert!(m.trivial_full_selection, "B covers hidden node ids by selecting all declared nodes");
        let w = honesty_warnings(&m);
        assert!(w.iter().any(|x| x.contains("trivial_full_selection")));
        assert!(m.hidden_only_count > 0);
        assert!((m.hidden_only.recall - 1.0).abs() < 1e-9, "B also 'detects' hidden-only ids by over-selection");
    }

    #[test]
    fn selective_misses_hidden_only_nodes() {
        let s = generate(7, 32, ScenarioFamily::HiddenDependency);
        let declared = s.declared_graph();
        let pred = run_strategy(RevalidationStrategy::Selective, &declared, &s.changes);
        let m = evaluate("selective", &pred, &affected(&s), &affected_declared(&s), &declared, 0);
        assert!(!m.trivial_full_selection);
        assert!(m.hidden_only_count > 0);
        assert_eq!(m.hidden_only.recall, 0.0);
        assert!(m.true_impact.recall < 1.0);
        assert!((m.registered_impact.recall - 1.0).abs() < 1e-9 || m.registered_impact.vacuous);
    }

    #[test]
    fn vacuous_truth_is_flagged() {
        let s = generate(3, 16, ScenarioFamily::UnrelatedChange);
        let declared = s.declared_graph();
        let pred = run_strategy(RevalidationStrategy::None, &declared, &s.changes);
        let m = evaluate("none", &pred, &affected(&s), &affected_declared(&s), &declared, 0);
        assert!(m.true_impact.vacuous);
        assert!((m.true_impact.recall - 1.0).abs() < 1e-9);
        assert!(honesty_warnings(&m).iter().any(|x| x.contains("vacuous")));
    }
}
