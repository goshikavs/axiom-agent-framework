#[cfg(test)]
mod tests {
    use goshikq_core::{run_strategy, RevalidationStrategy};

    use crate::generate::{generate, ScenarioFamily};

    #[test]
    fn four_strategies_diverge_on_hidden_dependency() {
        let s = generate(11, 32, ScenarioFamily::HiddenDependency);
        let g = s.declared_graph();
        let a = run_strategy(RevalidationStrategy::None, &g, &s.changes);
        let b = run_strategy(RevalidationStrategy::FullDeclared, &g, &s.changes);
        let c = run_strategy(RevalidationStrategy::ConventionalPrewrite, &g, &s.changes);
        let d = run_strategy(RevalidationStrategy::Selective, &g, &s.changes);
        assert!(a.decisions_to_recheck.is_empty() && a.actions_to_block.is_empty());
        assert!(b.decisions_to_recheck.len() > d.decisions_to_recheck.len());
        assert!(
            c.decisions_to_recheck.is_empty(),
            "conventional does not walk to decisions"
        );
        assert!(!d.decisions_to_recheck.is_empty());
        assert_ne!(a, b);
        assert_ne!(b, d);
        assert_ne!(c, d);
    }
}
