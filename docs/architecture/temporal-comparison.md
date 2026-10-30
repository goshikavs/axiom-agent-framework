# Temporal comparison (architecture review)

**v0.7 decision: Temporal adapter is deferred.** It is not a dependency of `axiom-server`. Native `axiom-runtime` remains the only compiled executor.

## Why defer

Axiom already persists checkpoints, action journals, leases, approvals, and `WAITING_FOR_EXTERNAL_STATE`. Adding Temporal as a second authoritative state machine would split goal verification, evidence, and Studio interpretation.

Windows local development also has no Temporal cluster in this repo. Introducing one for a timer/scheduler slice would not unlock parent/child verification, which is the v0.7 differentiator.

## What must stay native

| Concern | Owner |
|---|---|
| Goal contracts, predicates, outcome contract | `axiom-core` + worker `goal.evaluate` |
| Action-effect / retry safety / uncertain writes | journal + `EffectCategory` |
| Evidence graph, coverage, revalidation | existing tables + `axiom-core` |
| Parent/child tasks, shared context | migration `0005`, native APIs |
| Studio event interpretation | persisted `execution_events` |

## What Temporal could still add later (optional)

- Durable timers for long external waits across hosts
- Multi-host worker scheduling
- Cluster-level workflow history for *scheduling*, not verification

If implemented, Temporal Workflow code must not contain model calls or side effects. Those stay activities that call the existing Axiom execution protocol. One run still has one journal.

## Sketch (unimplemented)

```text
trait StepExecutor {
    fn start_run(req) -> RunId;
    fn signal_approval(run, approval_id, decision);
    fn signal_external_settled(run, operation_id);
}

NativeExecutor: current axiom-runtime   // default, compiled
TemporalExecutor: not in this tree
```

Do not add a second event log, lease table, or Temporal-specific goal type.
