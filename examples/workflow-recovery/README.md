# Workflow recovery after restart

Same graph as `examples/visual-workflow/parallel-approval.json`. The recovery harness kills `goshikq-server` while the run is `WAITING_FOR_APPROVAL`, starts it again, then approves.

```text
python -m pytest tests/workflows/test_workflow_recovery.py -q
```

Expected:

- Node `start` stays `completed` across the restart (not re-executed).
- The pending approval row is still listed.
- After approve, the run is `COMPLETED` and all eight nodes are `completed`.

This uses a dedicated server subprocess. Do not treat a Studio screenshot as recovery proof.
