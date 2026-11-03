# Visual workflow (parallel + approval)

Pinned JSON executed by the Rust runtime, not by Studio.

```text
goshikq workflows validate examples/visual-workflow/parallel-approval.json
goshikq workflows publish examples/visual-workflow/parallel-approval.json
goshikq workflows run <workflow-id>
```

Expected:

1. Run status becomes `WAITING_FOR_APPROVAL` after the parallel inspect/eligibility branches join.
2. Approve in Studio `/approvals` or `POST /api/v1/approvals/{id}/decision`.
3. Run `COMPLETED`. `GET /api/v1/runs/{id}/workflow-nodes` shows eight `completed` nodes.

AGENT nodes are **native** (no child LLM worker) unless `config.spawn` is `true`. TOOL nodes use the native `echo` / declared `result` handlers only.

Tests: `tests/workflows/test_workflow_execution.py`.
