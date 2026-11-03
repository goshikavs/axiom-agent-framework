# GitHub search discoverability audit

**Date:** 2026-11-03  
**Repository:** https://github.com/goshikavs/goshikq-agent-framework  
**Docs URL (intended):** https://goshikavs.github.io/goshikq-agent-framework/

A `site:github.com/goshikavs/goshikq-agent-framework` query with no hits is **not** treated as indexed. This audit does not claim Google has crawled the repository.

## Findings at audit time

| Item | State |
| --- | --- |
| Visibility | Public |
| GitHub description | Present; later aligned with README terms |
| Homepage URL | Pointed at the pre-rename `axiom-agent-framework` GitHub URL; now `https://goshikavs.github.io/goshikq-agent-framework/` |
| GitHub Pages | Was not enabled (`GET /pages` → 404). API now sets `build_type=workflow`; first deploy still needs `.github/workflows/docs.yml` on `main`. |
| Topics | Had `ai-agents`, `observability`, `opentelemetry`, `python`, `rust`, `typescript`; extra topics applied (see below) |
| README H1 | Keyword-stuffed; still linked the old repository name |
| Docs site | VitePress at `docs/site`, built in CI, `ignoreDeadLinks: true`, no sitemap/robots/canonical |
| Canonical GitHub URL | Mix of `axiom-agent-framework` and `goshikq-agent-framework` |

Renaming a GitHub repository starts a **new** URL for crawlers. Redirects from `axiom-agent-framework` do not make the new path appear in `site:` results until Google recrawls.

## Changes made in-tree

- README intro rewritten with natural terms (AI agent framework, Rust runtime, durable execution, multi-agent orchestration, verified outcomes) without production-readiness claims.
- Active self-links updated from `axiom-agent-framework` to `goshikq-agent-framework`. Historical audit text notes the later rename instead of rewriting the original record.
- VitePress: `base`, page titles, meta descriptions, canonical URLs, `sitemap.xml` (hostname includes the trailing project-site path), `robots.txt` via Vite `publicDir`, local search, GitHub edit links.
- GitHub Actions workflow `.github/workflows/docs.yml` deploys the VitePress `dist` to GitHub Pages from `main`.
- Guide: [Search Console](../guides/search-console.md).

## Actions that need the owner

1. Commit and merge this branch to `main` so `.github/workflows/docs.yml` can publish Pages. Enabling Pages does not deploy until that workflow runs.
2. Confirm Settings → Pages shows Source: **GitHub Actions**.
3. Add Search Console properties and paste a `google-site-verification` token when Google issues one.
4. Request indexing only after `https://goshikavs.github.io/goshikq-agent-framework/` returns HTTP 200.

Google indexing is **not** confirmed. Empty `site:` results should still be treated as not indexed.

## Recommended GitHub topics

Applied (14, max 20): `ai-agents`, `agent-framework`, `rust`, `python`, `typescript`, `llm`, `multi-agent`, `durable-execution`, `opentelemetry`, `observability`, `mcp`, `postgresql`, `workflow`, `verification`.
