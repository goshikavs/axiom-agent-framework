# Studio

Goshikq Studio is a developer operations console. It reads persisted runs from `goshikq-server` through a Next.js proxy (`/api/runtime`).

Implemented: overview metrics, URL-state execution explorer, execution detail (synchronized graph/timeline/inspector), live SSE with reconnect/pause, verification report, evidence inspector with snapshot diffs, historical replay (GET-only), run comparison, failure analysis, approvals, agents, events, traces, state inspector, models, integrations, settings, cost breakdown, **multi-agent panel** (`GET /api/v1/runs/{id}/hierarchy`, persisted `run_tasks` only).

v0.5 pages: [execution-explorer.md](execution-explorer.md), [execution-debugging.md](execution-debugging.md), [replay.md](replay.md), [run-comparison.md](run-comparison.md). Screenshots: [screenshots/](screenshots/).

Planned pages are labeled **Planned** and do not call fake APIs.

Dev: `cd apps/studio && bun install && bun run dev` → http://127.0.0.1:3000
Playwright: `bunx playwright test` with Studio already on :3000 (`PLAYWRIGHT_SKIP_WEBSERVER=1`).
