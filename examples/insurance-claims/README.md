# Insurance claims (approval gate)

Synthetic claim CLM-77. `authorize_payout` is `NON_IDEMPOTENT_WRITE` with `requires_approval=True`. The runtime enters `WAITING_FOR_APPROVAL`; approve or deny in Studio. A model sentence is never treated as a paid claim.

```bash
python main.py
python main.py --mutate-policy   # POL-1 v4→v5 while approval is pending
```

Then open `http://127.0.0.1:3000/approvals`. After a mutate-policy run, the original approval is invalidated and `/executions/{id}/evidence` shows the v4→v5 impact. The payout is not dispatched.
