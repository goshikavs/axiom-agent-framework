# Evidence-aware agents

Register what a decision depended on. Goshikq will notice when that source version changes.

```python
from goshikq_ai import DecisionRef, Evidence, with_evidence

@tool(effect="read_only")
def check_coverage(claim_id: str) -> dict:
    policy = load_policy(claim_id)
    return with_evidence(
        {"covered": policy.coverage, "reserve_cents": policy.reserve_cents},
        [Evidence(source_system="policy-admin", resource_id=f"policy:{policy.id}", resource_version=str(policy.version))],
        DecisionRef(
            id=f"dec:coverage:{claim_id}",
            kind="coverage",
            depends_on=[f"policy:{policy.id}"],
            action="act:authorize_payout",
            payload={"resource_id": f"policy:{policy.id}", "amount_cents": 15000},
        ),
    )
```

On the `StateProvider` snapshot, include `_resources`:

```python
return StateSnapshot(state={
    "policies": ...,
    "_resources": [{"resource_id": "policy:POL-1", "version": "4", "source_system": "policy-admin"}],
}, version="4", source="claims-memory")
```

Do not attach full policy documents when a version or fingerprint is enough.

## Studio

Open `/executions/{run_id}/evidence` for the inspector, dependency graph, change-impact, and revalidation history. The execution timeline is unchanged.

## Limitations

- Dependencies the model considered but never registered are invisible.
- External writes and Goshikq’s graph are not one distributed transaction.
- Concurrent source updates are detected when the snapshot carries a version/ETag; lost updates without versions are not.
