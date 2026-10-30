# Evidence coverage contracts

A coverage contract lists **declared** requirements for an action. Evaluating it answers only: were those requirements present, fresh, and non-conflicting?

It does **not** prove that every fact a model internally considered is known. Completing a graph walk or a coverage check never, by itself, marks a business goal `VERIFIED`.

## Status values

| Status | Meaning |
|---|---|
| `COMPLETE_FOR_DECLARED_CONTRACT` | Every listed requirement had a covering fact that met trust, version, and freshness rules |
| `INCOMPLETE` | A listed requirement had no covering fact |
| `STALE` | A listed requirement’s recorded version is not current, or a matching change event hit it |
| `CONFLICTED` | Covering facts disagree on version or fingerprint |
| `INCONCLUSIVE` | Inaccessible evidence, missing versions, or an unsatisfied verification method |

Coverage status is stored separately from `verification_status`.

## Python (SDK)

```python
@tool(
    effect="non_idempotent_write",
    risk="critical",
    required_evidence=["policy:POL-1", "order:{order_id}"],
    required_invariants=["coverage"],
    on_missing_evidence="block",
    revalidation_policy="strict_contract",
)
async def authorize_payout(claim_id: str, amount_cents: int) -> dict:
    ...
```

`order:{order_id}` is bound from the tool-call arguments before evaluation. Dotted keys such as `policy.coverage` match the `policy:` resource family and optionally a decision kind.

If `required_evidence` is set and `revalidation_policy` is omitted, the SDK defaults the policy to `STRICT_CONTRACT`. Existing tools without a contract stay on `SELECTIVE` (v0.3 behavior).

## Evaluator

IO-free: `axiom_core::evaluate_coverage`. Inputs are facts, decision facts, resource changes, and `now`. Reasons name missing, stale, conflicted, and inconclusive requirement keys.

## Limitation

A developer who omits a real business dependency from the contract will still get `COMPLETE_FOR_DECLARED_CONTRACT`. That is a complete declared list, not a complete world.
