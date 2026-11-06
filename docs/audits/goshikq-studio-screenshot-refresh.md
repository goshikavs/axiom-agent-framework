# Goshikq Studio screenshot refresh

Date: 2026-11-06  
Branch: `docs/studio-screenshot-refresh`  
Scope: documentation images only. Runtime verification semantics were not changed.

## Why

The product was renamed from Axiom to Goshikq. Markdown still pointed at Axiom-era Studio PNGs (`docs/studio/screenshots/*.png` and `docs/assets/studio-*.png`). Those files showed Axiom chrome and, in some cases, older navigation.

This refresh recaptured every documentation screenshot from the **current running** Goshikq Studio (`apps/studio`) against `goshikq-server` 0.1.0-alpha.

## Phase 1 — inventory (before)

### Tracked images on `origin/main`

| Path | Referenced from | Disposition |
| --- | --- | --- |
| `docs/studio/screenshots/overview.png` | `docs/studio/overview.md`, `docs/milestones/v0.5-report.md` | Replaced by `docs/assets/studio/goshikq-studio-dashboard.webp`; file deleted |
| `docs/studio/screenshots/executions.png` | `docs/studio/execution-explorer.md`, v0.5 report | Replaced by `goshikq-execution-explorer.webp`; deleted |
| `docs/studio/screenshots/executions-langgraph.png` | execution-explorer, v0.5 report | Replaced by `goshikq-execution-explorer-langgraph.webp`; deleted |
| `docs/studio/screenshots/execution-detail.png` | execution-debugging, v0.5 report | Replaced by `goshikq-execution-detail.webp`; deleted |
| `docs/studio/screenshots/replay.png` | replay.md, v0.5 report | Replaced by `goshikq-execution-replay.webp`; deleted |
| `docs/studio/screenshots/compare.png` | run-comparison.md, v0.5 report | Replaced by `goshikq-run-comparison.webp`; deleted |
| `docs/studio/screenshots/integrations.png` | v0.5 report | Replaced by `goshikq-integrations.webp`; deleted |
| `docs/assets/studio-hierarchy-refund.png` | README.md, v0.8 report | Replaced by `goshikq-multi-agent-hierarchy.webp`; deleted |
| `docs/assets/studio-workflows.png` | README.md, v0.8 report | Replaced by `goshikq-workflow-builder.webp`; deleted |
| `docs/assets/studio-workflow-run.png` | README.md, v0.8 report | Replaced by `goshikq-workflow-run.webp`; deleted |

No JPG, JPEG, SVG, GIF, or MDX screenshot references were found. Unrelated images were not deleted.

### Markdown / MDX references (before)

- `README.md` — three `docs/assets/studio-*.png` embeds
- `docs/studio/{overview,execution-explorer,execution-debugging,replay,run-comparison}.md` — backtick or missing image paths under `docs/studio/screenshots/`
- `docs/milestones/v0.5-report.md`, `v0.8-report.md` — path lists
- `apps/studio/e2e/workflows.spec.ts` — previously wrote docs PNGs during CI (removed; CI no longer mutates docs assets)

## Phase 2 — running Studio

Live stack used for capture:

| Service | Address | Notes |
| --- | --- | --- |
| Studio | `http://127.0.0.1:3000` | Next.js 15.5, `bun run studio` |
| Runtime | `http://127.0.0.1:7400` | `goshikq-server.exe` 0.1.0-alpha, `auth_mode=none` |
| Database | embedded Postgres | `GOSHIKQ_EMBEDDED_PG_DIR=.goshikq/postgres-docs`, port 55433. The developer `.axiom/postgres` cluster on 55432 was left intact because migration checksums on that cluster do not match the current binary |

Implemented screens captured: Overview, Executions, Execution detail (graph / timeline / inspector), Verification, Evidence, Workflows, Approvals, Replay, Compare, Failure analysis, Integrations, Models, Settings, Agents, plus a claims run paused for approval.

Not captured as working features: Tools, Goals, Evaluations, Regression Tests, Benchmarks (nav labeled **soon**).

Small Studio copy fix included so screenshots are not wrong: “an Goshikq” → “a Goshikq” on the execution page. `sdk-origin.ts` maps both `axiom-*` and `goshikq-*` source_framework values as native so a mixed historical database does not mislabel native SDKs. Verification rules were not changed.

## Phase 3 — capture

Script: `apps/studio/scripts/capture-docs-screenshots.ts`  
Seed: `apps/studio/scripts/seed-docs-fixtures.py`  
Command: `bun run screenshots:docs`

Capture settings: viewport 1600×1000, `deviceScaleFactor` 2, WebP quality 90, Next.js badge hidden, React Flow minimap/controls hidden so graph nodes are not covered, fonts awaited, SSE `reconnecting` and Monaco `Loading…` waited out.

Fixtures observed on the docs cluster:

| Agent / workflow | Outcome shown |
| --- | --- |
| `hello-agent` | `COMPLETED` / `VERIFIED` against `greetings-memory` |
| `refund-supervisor` | `VERIFIED` with four `VERIFIED` children |
| LangGraph OTLP `docs-langgraph-lookup` | `UNVERIFIED` / `NOT_EVALUATED` |
| `pw-docs-workflow-*` | `WAITING_FOR_APPROVAL` / `NOT_EVALUATED` |
| `claims_agent` | `WAITING_FOR_APPROVAL` / `NOT_EVALUATED` |

### New files

| File | What it shows |
| --- | --- |
| `goshikq-studio-dashboard.webp` | Overview metrics from persisted runs |
| `goshikq-execution-explorer.webp` | hello-agent filter; native `VERIFIED` rows |
| `goshikq-execution-explorer-langgraph.webp` | Imported LangGraph stays unverified |
| `goshikq-execution-detail.webp` | Graph, timeline, inspector JSON loaded |
| `goshikq-verification-report.webp` | Independent outcome fields; predicate evidence |
| `goshikq-evidence-inspector.webp` | Snapshot before/after; empty evidence list when tools did not attach `_evidence` |
| `goshikq-multi-agent-hierarchy.webp` | Persisted `run_tasks` children |
| `goshikq-workflow-builder.webp` | React Flow editor for a published graph |
| `goshikq-workflow-run.webp` | Node-level status, waiting for approval |
| `goshikq-approval-queue.webp` | Two pending approvals, actor `local-user` |
| `goshikq-execution-replay.webp` | GET-only replay of hello-agent |
| `goshikq-run-comparison.webp` | hello-agent vs refund-supervisor; unmatched events labeled |
| `goshikq-failure-analysis.webp` | LangGraph row with no recorded failure reason |
| `goshikq-integrations.webp` | goshikq-python / goshikq-typescript / langgraph cards |
| `goshikq-models.webp` | Mock pricing and adapter JSON |
| `goshikq-settings.webp` | Local health JSON, `auth_mode: none` |
| `goshikq-agents.webp` | Versioned definitions from submitted runs |
| `goshikq-claims-execution.webp` | claims_agent recovery panel while waiting for approval |
| `manifest.json` | Capture metadata (relative paths, viewport, fixture run ids) |

## Phase 4 — Markdown

Updated:

- `README.md`
- `docs/studio/overview.md` (gallery of implemented screens)
- `docs/studio/execution-explorer.md`
- `docs/studio/execution-debugging.md`
- `docs/studio/replay.md`
- `docs/studio/run-comparison.md`
- `docs/guides/workflow-builder.md`
- `docs/guides/bun-development.md`
- `docs/guides/studio-screenshots.md` (new)
- `docs/milestones/v0.5-report.md`
- `docs/milestones/v0.8-report.md`
- `docs/site/.vitepress/config.ts` (sidebar link)

Alt text and italic captions describe the persisted fixture, not a marketing claim.

## Phase 5 — visual validation

| Check | Result |
| --- | --- |
| Axiom Studio chrome | None on captured pages (script fails if `Axiom Studio` is visible) |
| Product name | Sidebar **Goshikq Studio**; frameworks `goshikq-python` / `goshikq-workflow` |
| Broken image links | Old PNG paths removed after replacements existed |
| Loading frames | Next.js badge, Monaco `Loading…`, and SSE `reconnecting` waited out |
| Fake success | LangGraph remains `UNVERIFIED`; workflow/claims remain `WAITING_FOR_APPROVAL` |
| Secrets | No API keys. Settings shows `auth_mode: none`. Models JSON names the `OPENAI_API_KEY` env var only |
| Clipped controls | React Flow minimap/controls hidden for capture so graph nodes stay readable |
| Mobile screenshots | Not taken; Studio has no separate mobile product layout |
| Playwright visual baselines | None existed (`toHaveScreenshot` unused). No snapshot files to update |
| Authentic UI quirks kept | Overview average duration prints a long float; replay step 1 has sparse JSON; hello-agent evidence list is empty because tools did not attach `_evidence`; in-flight workflow/claims pages may show an amber SSE `reconnecting` badge |

Playwright e2e still targets behavior, not docs assets (`apps/studio/e2e/workflows.spec.ts`).

## Phase 6 — maintenance

- Root: `bun run screenshots:docs`
- Seed: `bun run screenshots:docs:seed`
- Guide: [docs/guides/studio-screenshots.md](../guides/studio-screenshots.md)

## Non-goals / not done

- No GitHub release
- No rewrite of historical git commits
- Developer `.axiom/postgres` on 55432 was not migrated (checksum mismatch with current `goshikq-server`)
- Planned Studio pages were not photographed as if they worked
