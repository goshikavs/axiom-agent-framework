# Insurance: policy v4 → v5 while approval is pending

```text
python examples/insurance-claims/main.py --mutate-policy
```

Without the flag, approve in Studio as in v0.2. With the flag:

1. Coverage is recorded against `policy:POL-1` version 4.
2. The agent proposes `authorize_payout` (15000¢). The run waits for approval.
3. After ~1.5s the simulated policy store moves POL-1 to version 5 (`reserve_cents=5000`).
4. A human (or test) still grants the original approval.
5. Axiom snapshots current state, computes impact, marks `dec:coverage:CLM-77` for revalidation, and **invalidates** the approval.
6. The payout is not dispatched. The run ends `FAILED` with a stale-decision reason.
7. `CLAIMS["CLM-77"]["payout_cents"]` stays 0.

Studio: `/executions/{id}/evidence` shows evidence v4, impact 4→5, revalidation INVALID, and `approval.invalidated` after `approval.granted` (the grant event is not rewritten).

Tests: `python -m pytest tests/evidence/test_evidence_scenarios.py::test_insurance_policy_change_invalidates_approval -v`
