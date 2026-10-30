//! Evidence-revalidation benchmark runner.
//!
//! Four independent strategies (A none, B full-declared, C conventional pre-write,
//! D selective) are scored against ground truth built from the true graph,
//! including hidden edges withheld from the algorithms.
//!
//! Reproduction:
//!
//! ```text
//! cargo run -p axiom-bench --release -- --sizes 10,100,1000,10000 --trials 10 --seed 42 --out benchmarks/evidence/results.json
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use axiom_core::{run_strategy_with_context, RevalidationStrategy, StrategyContext};
use clap::Parser;
use serde::Serialize;

use axiom_bench::generate::{generate, ScenarioFamily};
use axiom_bench::ground_truth::{affected, affected_declared};
use axiom_bench::host::{self, HostInfo};
use axiom_bench::metrics::{evaluate, summarize, StrategySummary, TrialMetrics};

#[derive(Parser, Debug)]
#[command(name = "axiom-bench", about = "Independent evidence-revalidation baselines")]
struct Args {
    /// Comma-separated graph sizes (evidence-node counts).
    #[arg(long, default_value = "10,100,1000,10000")]
    sizes: String,
    /// Seeded trials per (size, family).
    #[arg(long, default_value_t = 10)]
    trials: u32,
    /// Base seed. Trial t uses seed = base ^ (size << 16) ^ (family_idx << 8) ^ t.
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value = "benchmarks/evidence/results.json")]
    out: PathBuf,
    /// Restrict families (comma-separated). Default: all.
    #[arg(long)]
    families: Option<String>,
}

#[derive(Serialize)]
struct Cell {
    size: usize,
    family: String,
    trials: Vec<BTreeMap<String, TrialMetrics>>,
    summaries: Vec<StrategySummary>,
}

#[derive(Serialize)]
struct Report {
    generated_at: String,
    seed: u64,
    trials_per_cell: u32,
    strategies: [&'static str; 6],
    metric_definitions: BTreeMap<&'static str, &'static str>,
    host: HostInfo,
    cells: Vec<Cell>,
    overall: Vec<StrategySummary>,
    limitations: Vec<&'static str>,
}

const STRATEGIES: [RevalidationStrategy; 6] = [
    RevalidationStrategy::None,
    RevalidationStrategy::FullDeclared,
    RevalidationStrategy::ConventionalPrewrite,
    RevalidationStrategy::Selective,
    RevalidationStrategy::SelectivePlusPrewrite,
    RevalidationStrategy::StrictContract,
];

fn parse_sizes(s: &str) -> Vec<usize> {
    s.split(',')
        .filter_map(|p| p.trim().parse::<usize>().ok())
        .filter(|n| *n >= 4)
        .collect()
}

fn parse_families(s: Option<&str>) -> Vec<ScenarioFamily> {
    match s {
        None => ScenarioFamily::ALL.to_vec(),
        Some(raw) => {
            let wanted: Vec<&str> = raw.split(',').map(|x| x.trim()).collect();
            ScenarioFamily::ALL
                .into_iter()
                .filter(|f| wanted.iter().any(|w| *w == f.as_str()))
                .collect()
        }
    }
}

fn cell_seed(base: u64, size: usize, family_idx: usize, trial: u32) -> u64 {
    base ^ ((size as u64) << 16) ^ ((family_idx as u64) << 8) ^ (trial as u64)
}

fn main() {
    let args = Args::parse();
    let sizes = parse_sizes(&args.sizes);
    let families = parse_families(args.families.as_deref());
    assert!(!sizes.is_empty(), "no valid sizes");
    assert!(!families.is_empty(), "no valid families");

    let mut cells = Vec::new();
    let mut by_strategy: BTreeMap<String, Vec<TrialMetrics>> = BTreeMap::new();

    for size in sizes {
        for (family_idx, family) in families.iter().enumerate() {
            let mut trial_rows: Vec<BTreeMap<String, TrialMetrics>> = Vec::new();
            let mut strat_acc: BTreeMap<String, Vec<TrialMetrics>> = BTreeMap::new();
            for trial in 0..args.trials {
                let seed = cell_seed(args.seed, size, family_idx, trial);
                let scenario = generate(seed, size, *family);
                let truth = affected(&scenario);
                let registered = affected_declared(&scenario);
                let declared = scenario.declared_graph();
                let ctx = StrategyContext {
                    contracts: scenario.action_contracts.clone(),
                    facts: scenario.facts.clone(),
                    ..StrategyContext::default()
                };
                let mut row = BTreeMap::new();
                for strat in STRATEGIES {
                    let t0 = Instant::now();
                    let predicted = run_strategy_with_context(strat, &declared, &scenario.changes, &ctx);
                    let latency_ns = t0.elapsed().as_nanos();
                    let m = evaluate(strat.as_str(), &predicted, &truth, &registered, &declared, latency_ns);
                    strat_acc.entry(strat.as_str().to_string()).or_default().push(m.clone());
                    by_strategy.entry(strat.as_str().to_string()).or_default().push(m.clone());
                    row.insert(strat.as_str().to_string(), m);
                }
                trial_rows.push(row);
            }
            let summaries: Vec<StrategySummary> = STRATEGIES
                .iter()
                .map(|s| summarize(s.as_str(), strat_acc.get(s.as_str()).map(|v| v.as_slice()).unwrap_or(&[])))
                .collect();
            eprintln!(
                "size={size} family={} trials={} B_true={:.3} B_trivial={:.3} D_true={:.3} D_hidden={:.3} F_prevent={:.3} F_falseblk={:.3}",
                family.as_str(),
                args.trials,
                summaries[1].true_impact_recall.mean,
                summaries[1].trivial_full_selection_rate.mean,
                summaries[3].true_impact_recall.mean,
                summaries[3].hidden_detection_recall_nonvacuous.mean,
                summaries[5].action_prevention_recall_nonvacuous.mean,
                summaries[5].false_block_rate.mean,
            );
            cells.push(Cell {
                size,
                family: family.as_str().to_string(),
                trials: trial_rows,
                summaries,
            });
        }
    }

    let overall: Vec<StrategySummary> = STRATEGIES
        .iter()
        .map(|s| summarize(s.as_str(), by_strategy.get(s.as_str()).map(|v| v.as_slice()).unwrap_or(&[])))
        .collect();

    let report = Report {
        generated_at: chrono::Utc::now().to_rfc3339(),
        seed: args.seed,
        trials_per_cell: args.trials,
        strategies: [
            "none",
            "full_declared",
            "conventional_prewrite",
            "selective",
            "selective_plus_prewrite",
            "strict_contract",
        ],
        metric_definitions: [
            ("true_impact_recall", "Predicted node ids vs true-graph affected set (declared ∪ hidden). Vacuous when the truth set is empty."),
            ("registered_impact_recall", "Predicted node ids vs declared-graph affected set only."),
            ("hidden_detection_recall", "Predicted node ids vs nodes reachable only via hidden edges. Full-declared scores 1 by selecting every declared node (trivial_full_selection)."),
            ("action_prevention_recall", "Fraction of true-affected actions that were blocked."),
            ("incorrect_action_rate", "Trial-level 1 if any true-affected action was not blocked."),
            ("false_block_rate", "Fraction of predicted blocked actions that are not in the true-affected set."),
            ("trivial_full_selection", "1 when the strategy selected every declared decision and action."),
            ("verification_workload", "Count of predicted decision+action+approval ids."),
            ("latency_ns", "Wall-clock of the strategy function."),
        ]
        .into_iter()
        .collect(),
        host: host::collect(),
        cells,
        overall,
        limitations: vec![
            "Ground truth includes hidden edges withheld from A–E. STRICT_CONTRACT may block additional actions only when a developer-declared coverage item matches a changed resource.",
            "Full-declared true-impact recall of 1.0 is trivial_full_selection: hidden edges land on existing declared node ids.",
            "No strategy infers unregistered dependencies. STRICT cannot either, unless the contract lists the resource.",
            "A–D outputs are unchanged from v0.3.1; only metric labels and E/F policies were added.",
            "Approval/external delays are metadata. Database overhead N/A. Model cost unknown.",
            "Latency is in-process wall-clock of the strategy function, not end-to-end agent runtime.",
        ],
    };

    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).expect("create output dir");
        }
    }
    let json = serde_json::to_string_pretty(&report).expect("serialize report");
    fs::write(&args.out, json).expect("write report");
    eprintln!("wrote {}", args.out.display());
}
