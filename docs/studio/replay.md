# Historical replay

URL: `/replay`

Read-only. The viewer GETs persisted events and the current `/api/v1/runs/{id}/state` snapshot. Step forward/back changes which recorded event is highlighted.

It never:

- calls a model provider
- executes tools
- submits refunds
- grants approvals
- re-dispatches actions

Playwright records requests during replay and asserts no POST/PUT/PATCH/DELETE.

A controlled sandbox rerun would be a different UI with an explicit user action. That UI is not this page.

Screenshot: `docs/studio/screenshots/replay.png`.
