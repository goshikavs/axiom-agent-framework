# Website quality report

Branch: `feat/goshikq-public-website`. Status of checks is filled after they run in this change.

## Pages implemented

Marketing: `/`, `/features`, `/quickstart`, `/download`, `/studio`, `/architecture`, `/examples`, `/examples/refund-verification`, `/examples/insurance-claims`, `/community`. `/changelog` and `/roadmap` redirect into docs.

Documentation: Starlight under `/docs/**` including generated copies of installation, architecture, verification, evidence, recovery, multi-agent, workflows, Studio, integrations, security, operations, API, status, packaging, changelog, and roadmap.

## Claims policy

- Alpha `0.1.0-alpha` / v0.9.1 in tree.
- No PyPI, npm, crates.io, or GitHub Release packages.
- No SOC 2 / HIPAA / PCI.
- Studio Tools/Goals/Evaluations/Regression/Benchmarks not shown as shipped.
- Verification simulation is labeled as not a live runtime.

## Hosting cost (assumptions)

| Item | Plan | Notes |
| --- | --- | --- |
| Hosting | Cloudflare Pages free | Static; not unlimited in the legal sense — free-tier quotas apply |
| Domain | Not purchased | Optional later; registrar fees are extra |
| Build | GitHub Actions + Cloudflare builds | Free-tier minutes |
| Search | Pagefind static index | No Algolia |
| Analytics | None | Optional later; not required |
| Scaling | CDN | Traffic beyond free quotas would need a paid plan |

## Awaiting owner approval

- Cloudflare Pages connection and production deploy
- Custom domain / DNS
- Search Console verification token
- Switching GitHub About website URL
- Replacing VitePress GitHub Pages with this site

## Test results (this branch)

| Check | Result |
| --- | --- |
| `bun run build` in `apps/website` | Pass — 47 HTML pages, Pagefind index, `sitemap-index.xml` |
| Playwright desktop + mobile Chromium | 17 passed, 1 skipped (mobile-only nav test on desktop) |
| robots.txt | `Allow: /`, no sitewide Disallow |
| sitemap | `/sitemap-index.xml` 200 on preview |
| Download honesty | No PyPI/npm; ZIP is `main` snapshot |
| Axiom chrome | Not present on homepage |
| Lighthouse | Not run in CI. Treat 95+ scores as targets after Cloudflare preview. |
| Production deploy | Not performed |

Visual review of local preview (`http://127.0.0.1:4321`): homepage, Studio gallery, and Starlight `/docs/` sidebar/search. Studio thumbnails are dark product screenshots on a dark surface; they are real captures, not empty placeholders.
