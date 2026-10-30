# Evidence dependency graph

Implemented in `axiom-core::impact` (IO-free) and persisted in `evidence_nodes` / `evidence_edges`.

## Node kinds

Source, Evidence, Decision, Action, Goal predicate, Invariant, Approval.

## Edge kinds

`retrieved`, `supports`, `requires`, `authorizes`, `invalidates`, `derived_from`.

## What is stored

Only **explicitly registered** relationships. Tools attach `_evidence` and `_decision`. Snapshots attach `_resources`. Axiom does not claim to recover the model’s private chain of thought. Studio labels edges as `registered` (`inferred=false`).

## Example (insurance)

```
src:policy-admin --retrieved--> ev:policy:POL-1@4
ev:policy:POL-1@4 --supports--> dec:coverage:CLM-77
dec:coverage:CLM-77 --requires--> act:authorize_payout
dec:coverage:CLM-77 --authorizes--> appr:<id>
act:authorize_payout --requires--> pred:claim_paid
```

When `POL-1` is later observed at version 5, change-impact walks outgoing edges from the evidence node and classifies dependents as **requiring verification**.
