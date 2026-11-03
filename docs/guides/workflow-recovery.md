# Workflow recovery

Workflow runs use the same leases, checkpoints, and durable waits as single-agent runs.

- Restart `goshikq-server` while a run is `WAITING_FOR_APPROVAL` or `WAITING_FOR_EXTERNAL_STATE`. The supervisor reschedules the executor. Completed nodes are skipped.
- Non-idempotent tool nodes that were `running` at crash require operator reconciliation (`RECOVERY_REQUIRED`).
- Child agent runs keep `idempotency_key=wf:{parent}:{node}` so a replay does not spawn a second child.
- Published version pinning: `metadata.workflow.definition` on the run is the snapshot; later catalog edits do not apply.

Crash harness coverage for generic runs remains `tests/crash` (8 passed, including approval-after-restart). Workflow-specific restart: `python -m pytest tests/workflows/test_workflow_recovery.py -q`.
