# Evidence-aware verification (design)

## Problem

A decision made against policy v4 is not automatically valid against policy v5. v0.2 could re-snapshot after approval, but it did not record *which* resources a decision depended on, so Studio could not explain impact and a stale approval could still authorize a write if invariants happened to still hold.

## Model

Explicit records only. The agent/worker **registers** evidence and dependency edges. Axiom does not claim to reconstruct the model’s private chain of thought.

```
Source (policy:POL-1 @ v4)
  --retrieved--> Evidence
  --supports--> Decision (coverage)
  --authorizes--> Approval
  --requires--> Action (authorize_payout)
  --requires--> Goal predicate / invariant
```

When `POL-1` is observed at v5, change-impact walks dependents and marks them **requires verification**. It does not mark them false.

## API / schema extensions (additive)

- `evidence_records`, `evidence_nodes`, `evidence_edges`, `decision_records`, `impact_reports`
- `tool_invocations.recovery_disposition` (independent of dispatch state)
- `approvals.status = invalidated`
- Events: `evidence.recorded`, `evidence.changed`, `decision.created`, `decision.revalidation_required`, `decision.revalidated`, `approval.invalidated`, `impact.computed`
- `GET /api/v1/runs/{id}/evidence`

Python: `axiom_ai.evidence.Evidence`, `DecisionRef`. Tools/snapshots may attach `_evidence` / `_decision`.

## Revalidation planner (deterministic)

1. Diff recorded resource versions against the latest snapshot.
2. `compute_impact` (BTree-ordered BFS, cycle detection).
3. Affected pending approvals → `invalidated` (append event; do not rewrite history).
4. Affected pending writes → do not dispatch until invariants + coverage/eligibility pass on fresh evidence.
5. Missing/inaccessible evidence → `INCONCLUSIVE`, not `FAILED`.

Database row locks and leases remain in force. The graph is not a substitute for `FOR UPDATE` or fencing tokens.
