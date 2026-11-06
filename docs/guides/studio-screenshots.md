---
title: Studio screenshots
description: How to recapture Goshikq Studio documentation screenshots from the running application.
---

# Studio documentation screenshots

Screenshots under `docs/assets/studio/` are captured from a **running** Goshikq Studio against a live `goshikq-server`. They are not mockups, AI-generated UI, or painted-over stills.

Regenerate them when Studio chrome, navigation, or branding changes.

## Command

From the repository root, with Studio and the runtime already up:

```bash
bun run screenshots:docs:seed   # optional; writes deterministic fixtures
bun run screenshots:docs
```

Equivalent Studio-package commands:

```bash
cd apps/studio
bun run screenshots:docs:seed
bun run screenshots:docs
```

`apps/studio/scripts/capture-screenshots.cjs` forwards to the same capture script. Do not write images into `docs/studio/screenshots/`.

## Prerequisites

| Requirement | Notes |
| --- | --- |
| Bun | Workspace package manager |
| Python 3 + Pillow | Fixture seed and lossless/lossy WebP conversion (`pip install pillow`) |
| Playwright Chromium | Installed with `@playwright/test` in `apps/studio` |
| `goshikq-server` | `GET http://127.0.0.1:7400/health` must succeed |
| Studio | Next.js on `http://127.0.0.1:3000` (`bun run studio`) |
| PostgreSQL | Default local stack uses embedded Postgres. Docs captures in this refresh used `DATABASE_URL=embedded` and `GOSHIKQ_EMBEDDED_PG_DIR=.goshikq/postgres-docs` (gitignored) so a developer cluster is not overwritten |

The capture script **does not** start Postgres, the runtime, or Studio. If any of those are down it exits with an error instead of inventing images.

## Fixture setup

`bun run screenshots:docs:seed` (`apps/studio/scripts/seed-docs-fixtures.py`) writes:

- `examples/hello-agent` → `COMPLETED` / `VERIFIED`
- `examples/multi-agent-refund` → supervisor plus four persisted children
- a LangGraph OTLP import that stays `UNVERIFIED` / `NOT_EVALUATED`
- a published `parallel-approval` workflow run paused at `WAITING_FOR_APPROVAL`
- `examples/insurance-claims` left waiting for a human approval

Do not approve the pending requests before capture if the approval-queue screenshot should show a real queue.

## Viewport and files

| Setting | Value |
| --- | --- |
| Viewport | 1600 × 1000 |
| Device scale | 2 |
| Format | WebP quality 90 (PNG is an intermediate and is deleted) |
| Output | `docs/assets/studio/goshikq-*.webp` |
| Manifest | `docs/assets/studio/manifest.json` |

Mobile and tablet captures are omitted. Studio is a dense desktop operations console and does not ship a separate responsive product layout.

## Review after capture

1. Open each WebP. Confirm **Goshikq Studio** in the sidebar, not Axiom.
2. Confirm native runs that actually verified show `VERIFIED`; imported LangGraph rows stay `UNVERIFIED`.
3. Reject any frame that still shows Next.js “N”, Monaco `Loading…`, or SSE `reconnecting`.
4. Do not crop in secrets, API keys, or personal names. Local docs fixtures use `local-user` and mock/deterministic models.
5. Update Markdown only after the files exist. Inventory: `docs/audits/goshikq-studio-screenshot-refresh.md` (not published on the docs site).

## Secrets

- Capture against `GOSHIKQ_AUTH_MODE=none` or a disposable local key. Never paste production keys into Studio or `.env` used for docs.
- Settings screenshots include runtime health JSON (`auth_mode`, uptime). That is expected. They must not include provider credentials.
- Models JSON may name env vars such as `OPENAI_API_KEY`. That is adapter metadata, not a leaked value.

## What is not captured

Tools, Goals, Evaluations, Regression Tests, and Benchmarks remain labeled **soon** / **Planned**. Those pages are not documented as working features.
