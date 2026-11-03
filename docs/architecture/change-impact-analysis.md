# Change-impact analysis

`goshikq_core::impact::compute_impact(graph, changes) -> ImpactReport`

## Inputs

- Changed resource id
- Old version (optional)
- New version (`None` = deleted)
- Optional `inaccessible`
- Graph snapshot (BTree-ordered)

## Outputs

- Affected decisions
- Pending actions
- Goal predicates / invariants to recheck
- Approvals
- Reasons
- `cyclic` flag
- Classification: `REQUIRES_VERIFICATION` or `INCONCLUSIVE`

An affected decision is **not** automatically incorrect.

## Algorithms

1. Collapse duplicate changes per resource (last new version wins; inaccessible ORs).
2. Seed nodes whose `resource_id` matches and whose stored version differs (or is missing).
3. DFS along outgoing edges. Rec-stack cycle detection: a back-edge sets `cyclic` and does not recurse forever.
4. Partition visited nodes by kind. Node ids are BTree-sorted, so two permutations of the same change set yield the same report.

## Selective revalidation (`goshikq_core::revalidation`)

Deterministic planner. No LLM.

1. Identify affected pending decisions.
2. Determine predicates/invariants to recheck.
3. Caller retrieves a fresh snapshot.
4. Explicit rules (`coverage`, `eligibility`) run against the snapshot.
5. Mark VALID / INVALID / INCONCLUSIVE.
6. Invalidate previously granted approvals when material prerequisites changed.
7. Block sensitive writes until verification passes.
8. Append `revalidation_results` (do not rewrite historical events).

Graph checks do not replace `FOR UPDATE`, fencing leases, or tool-level authorization.
